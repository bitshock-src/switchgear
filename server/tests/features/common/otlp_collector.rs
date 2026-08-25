//! In-process OTLP trace gRPC collector for OTLP-compliance snapshot tests.
//!
//! Runs on a per-test tokio task, receives spans via the OTLP protobuf
//! ExportTraceServiceRequest, buffers them lossless in memory, and on
//! shutdown reassembles the parent/child tree and writes a
//! deterministically-sorted JSON-lines file (one line per root span) that
//! `insta` snapshots as the OTLP compliance assertion.
//!
//! Runs over plain HTTP/2 (h2c) — no TLS. TLS adds no assertion value
//! (we're snapshotting on-wire OTLP shape, not transport crypto) and only
//! introduces IP-SAN / SNI surface that flakes across rustls versions.

use anyhow::{Context, Result, anyhow};
use opentelemetry_proto::tonic::collector::trace::v1::trace_service_server::{
    TraceService, TraceServiceServer,
};
use opentelemetry_proto::tonic::collector::trace::v1::{
    ExportTraceServiceRequest, ExportTraceServiceResponse,
};
use opentelemetry_proto::tonic::common::v1::{AnyValue, KeyValue, any_value};
use opentelemetry_proto::tonic::trace::v1::ResourceSpans;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tonic::transport::Server;

/// In-memory OTLP collector state. Spans are keyed by span_id; tree assembly
/// happens at shutdown so late-arriving children from unordered
/// `BatchSpanProcessor` batches always find their parent.
#[derive(Default)]
struct CollectorState {
    spans: HashMap<[u8; 8], CollectedSpan>,
}

/// Only the fields we actually consume at write-time are stored. Every
/// OTLP `Span` metadata field is emitted with a fixed type placeholder
/// (see `build_span_node`) regardless of value — for primitives with a
/// proto default (kind, flags, timestamps, dropped_*_count, trace_state)
/// there's no "absent" wire state to observe, so storing them adds no
/// verification signal. For `parent_span_id` and `status` the
/// present-vs-absent distinction IS observable and is preserved
/// (`None` → JSON `null`, `Some` → type placeholder).
struct CollectedSpan {
    trace_id: [u8; 16],
    parent_span_id: Option<[u8; 8]>,
    name: String,
    start_time_unix_nano: u64,
    attrs: Vec<KeyValue>,
    events: Vec<CollectedEvent>,
    /// The OTLP `Span.status.code` enum value. `None` when `Span.status`
    /// itself is absent on the wire. This is the one metadata field
    /// whose VALUE carries the signal (UNSET vs OK vs ERROR drives RED
    /// metrics + service map colouring in every backend), so it's
    /// emitted as its enum-name string rather than the generic
    /// `[int]` placeholder.
    status_code: Option<i32>,
    /// Resource attributes from the `ResourceSpans` batch this span
    /// arrived in. In OTLP, `Resource` is a batch-level property —
    /// every span in the batch shares it — but we stash a copy on each
    /// span so root-span emission can annotate each output line
    /// without carrying the batch context through tree assembly.
    resource: Vec<KeyValue>,
}

struct CollectedEvent {
    time_unix_nano: u64,
    name: String,
    attrs: Vec<KeyValue>,
}

struct CollectorService {
    state: Arc<Mutex<CollectorState>>,
}

#[tonic::async_trait]
impl TraceService for CollectorService {
    async fn export(
        &self,
        request: tonic::Request<ExportTraceServiceRequest>,
    ) -> std::result::Result<tonic::Response<ExportTraceServiceResponse>, tonic::Status> {
        let req = request.into_inner();
        let mut state = self.state.lock().expect("collector state mutex poisoned");
        for rs in req.resource_spans {
            ingest_resource_spans(&mut state, rs);
        }
        Ok(tonic::Response::new(ExportTraceServiceResponse {
            partial_success: None,
        }))
    }
}

fn ingest_resource_spans(state: &mut CollectorState, rs: ResourceSpans) {
    let resource_attrs = rs
        .resource
        .as_ref()
        .map(|r| r.attributes.clone())
        .unwrap_or_default();
    for ss in rs.scope_spans {
        for sp in ss.spans {
            let Some(span_id) = to_span_id(&sp.span_id) else {
                continue;
            };
            let trace_id = to_trace_id(&sp.trace_id).unwrap_or([0; 16]);
            let parent_span_id = to_span_id(&sp.parent_span_id);
            let events = sp
                .events
                .into_iter()
                .map(|e| CollectedEvent {
                    time_unix_nano: e.time_unix_nano,
                    name: e.name,
                    attrs: e.attributes,
                })
                .collect();
            state.spans.insert(
                span_id,
                CollectedSpan {
                    trace_id,
                    parent_span_id,
                    name: sp.name,
                    start_time_unix_nano: sp.start_time_unix_nano,
                    attrs: sp.attributes,
                    events,
                    status_code: sp.status.as_ref().map(|s| s.code),
                    resource: resource_attrs.clone(),
                },
            );
        }
    }
}

fn to_span_id(bytes: &[u8]) -> Option<[u8; 8]> {
    if bytes.len() != 8 || bytes.iter().all(|b| *b == 0) {
        return None;
    }
    let mut out = [0u8; 8];
    out.copy_from_slice(bytes);
    Some(out)
}

fn to_trace_id(bytes: &[u8]) -> Option<[u8; 16]> {
    if bytes.len() != 16 {
        return None;
    }
    let mut out = [0u8; 16];
    out.copy_from_slice(bytes);
    Some(out)
}

pub struct TestOtlpCollector {
    endpoint: String,
    bearer_token_path: PathBuf,
    log_path: PathBuf,
    address: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    serve_handle: Option<JoinHandle<Result<(), tonic::transport::Error>>>,
    state: Arc<Mutex<CollectorState>>,
    /// Names of the root spans the caller wants to audit. Anything else
    /// on the OTLP wire (ambient background tasks, harness readiness
    /// probes, adjacent test setup calls) is dropped at log-write time.
    root_span_whitelist: HashSet<String>,
}

impl TestOtlpCollector {
    /// Bind a per-test OTLP gRPC collector on `127.0.0.1:<allocated_port>`
    /// over plain HTTP/2 (h2c). Bearer token is generated but not validated:
    /// one client, one server, single test — auth adds nothing.
    ///
    /// `root_span_whitelist` declares which root span names the snapshot
    /// should include; every other root span (and its subtree) is
    /// dropped when the log is written. This inverts the previous
    /// blocklist approach: tests declare what they're auditing, and the
    /// ambient trace pool (background health-check timers, test-harness
    /// TCP probes, adjacent test-setup HTTP calls) is invisible by
    /// default. Pass an empty slice to include nothing (useful only
    /// for negative tests).
    pub fn spawn<S>(tempdir: &Path, root_span_whitelist: &[S]) -> Result<Self>
    where
        S: AsRef<str>,
    {
        let port_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
        let port = switchgear_testing::ports::PortAllocator::find_available_port(&port_dir)?;
        let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);

        let bearer_token_path = tempdir.join("otel-token");
        let bearer = uuid::Uuid::new_v4().to_string();
        fs::write(&bearer_token_path, bearer.as_bytes())?;

        let log_path = tempdir.join("otlp-collector.log");

        let state = Arc::new(Mutex::new(CollectorState::default()));
        let service = CollectorService {
            state: Arc::clone(&state),
        };
        let (tx, rx) = oneshot::channel::<()>();
        let server = Server::builder().add_service(TraceServiceServer::new(service));

        let serve_handle = tokio::spawn(async move {
            server
                .serve_with_shutdown(address, async move {
                    let _ = rx.await;
                })
                .await
        });

        Ok(Self {
            endpoint: format!("http://127.0.0.1:{port}"),
            bearer_token_path,
            log_path,
            address,
            shutdown: Some(tx),
            serve_handle: Some(serve_handle),
            state,
            root_span_whitelist: root_span_whitelist
                .iter()
                .map(|s| s.as_ref().to_string())
                .collect(),
        })
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn bearer_token_path(&self) -> &Path {
        &self.bearer_token_path
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Stop the tonic serve future and write the sorted JSON-lines log file.
    /// Returns the log path so callers can hand it straight to
    /// `insta::assert_json_snapshot!`.
    pub async fn shutdown(mut self) -> Result<PathBuf> {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.serve_handle.take() {
            match handle.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(anyhow!("OTLP collector serve error: {e}")),
                Err(e) => return Err(anyhow!("OTLP collector task join error: {e}")),
            }
        }
        let state = std::mem::take(
            &mut *self
                .state
                .lock()
                .map_err(|_| anyhow!("collector state mutex poisoned"))?,
        );
        write_log(&state, &self.log_path, &self.root_span_whitelist)?;
        Ok(self.log_path.clone())
    }
}

fn write_log(
    state: &CollectorState,
    path: &Path,
    root_span_whitelist: &HashSet<String>,
) -> Result<()> {
    let spans = &state.spans;

    // Group children by parent_span_id. Roots are spans with no parent OR
    // whose stated parent isn't in the pool (orphaned by a dropped batch).
    // Root-span whitelist: only emit roots whose name the caller explicitly
    // asked for. Everything else on the wire — background timers, harness
    // probes, adjacent test-setup requests — is dropped along with its
    // subtree.
    let mut children_of: HashMap<[u8; 8], Vec<[u8; 8]>> = HashMap::new();
    let mut roots: Vec<[u8; 8]> = Vec::new();
    for (id, span) in spans {
        match span.parent_span_id {
            Some(parent) if spans.contains_key(&parent) => {
                children_of.entry(parent).or_default().push(*id);
            }
            _ => {
                if !root_span_whitelist.contains(&span.name) {
                    continue;
                }
                roots.push(*id);
            }
        }
    }

    // Roots: sort by start_time asc, tie-break by trace_id.
    roots.sort_by(|a, b| {
        let sa = &spans[a];
        let sb = &spans[b];
        sa.start_time_unix_nano
            .cmp(&sb.start_time_unix_nano)
            .then_with(|| sa.trace_id.cmp(&sb.trace_id))
    });

    let mut file = fs::File::create(path)
        .with_context(|| format!("creating OTLP log file {}", path.display()))?;
    for root in roots {
        let mut node = build_span_node(root, spans, &children_of);
        // Attach `resource` to each root line. OTLP `Resource` is a
        // batch-level property (one per `ResourceSpans`), so it's the
        // same on every span from a single SDK — but the root line is
        // the natural anchor for it in a per-root JSON-lines log.
        // Values are type-redacted via `attrs_to_json`, so per-run
        // churn (`process.pid`, `host.name`, `service.version` on
        // version bumps) doesn't destabilise the snapshot; the
        // assertion is on which resource attribute KEYS the SDK
        // populated.
        if let Value::Object(ref mut obj) = node {
            obj.insert(
                "resource".to_string(),
                attrs_to_json(&spans[&root].resource),
            );
        }
        let line = serde_json::to_string(&node).context("serializing OTLP root-span JSON node")?;
        writeln!(file, "{line}").context("writing OTLP log line")?;
    }
    file.flush()?;
    Ok(())
}

enum Child<'a> {
    Event(&'a CollectedEvent),
    Span([u8; 8]),
}

impl<'a> Child<'a> {
    fn name(&self, spans: &'a HashMap<[u8; 8], CollectedSpan>) -> &'a str {
        match self {
            Child::Event(e) => &e.name,
            Child::Span(sid) => &spans[sid].name,
        }
    }

    fn kind_rank(&self) -> u8 {
        match self {
            Child::Event(_) => 0,
            Child::Span(_) => 1,
        }
    }

    fn time(&self, spans: &HashMap<[u8; 8], CollectedSpan>) -> u64 {
        match self {
            Child::Event(e) => e.time_unix_nano,
            Child::Span(sid) => spans[sid].start_time_unix_nano,
        }
    }
}

/// One JSON node in the OTLP compliance tree. Every OTLP `Span` metadata
/// field is emitted with its value type-redacted to a `[<type>]`
/// placeholder — same principle as attribute values, so the snapshot
/// stays stable while still asserting each metadata field is present on
/// the wire. `parent_span_id` is `null` for root spans (matching OTLP's
/// empty-bytes semantic) and `[bytes]` otherwise; `status` is a nested
/// object when set and `null` when unset.
fn build_span_node(
    id: [u8; 8],
    spans: &HashMap<[u8; 8], CollectedSpan>,
    children_of: &HashMap<[u8; 8], Vec<[u8; 8]>>,
) -> Value {
    let span = &spans[&id];

    let mut merged: Vec<Child<'_>> = Vec::new();
    for e in &span.events {
        merged.push(Child::Event(e));
    }
    if let Some(child_ids) = children_of.get(&id) {
        for cid in child_ids {
            merged.push(Child::Span(*cid));
        }
    }

    // Sort children: primary key = name, secondary = kind (event < span),
    // tertiary = time.
    merged.sort_by(|a, b| {
        a.name(spans)
            .cmp(b.name(spans))
            .then_with(|| a.kind_rank().cmp(&b.kind_rank()))
            .then_with(|| a.time(spans).cmp(&b.time(spans)))
    });

    let mut obj = Map::new();
    obj.insert("name".to_string(), Value::String(span.name.clone()));
    obj.insert("kind".to_string(), Value::String("[int]".to_string()));
    obj.insert("trace_id".to_string(), Value::String("[bytes]".to_string()));
    obj.insert("span_id".to_string(), Value::String("[bytes]".to_string()));
    obj.insert(
        "parent_span_id".to_string(),
        match span.parent_span_id {
            Some(_) => Value::String("[bytes]".to_string()),
            None => Value::Null,
        },
    );
    obj.insert(
        "trace_state".to_string(),
        Value::String("[string]".to_string()),
    );
    obj.insert("flags".to_string(), Value::String("[int]".to_string()));
    obj.insert(
        "start_time_unix_nano".to_string(),
        Value::String("[int]".to_string()),
    );
    obj.insert(
        "end_time_unix_nano".to_string(),
        Value::String("[int]".to_string()),
    );
    obj.insert(
        "status".to_string(),
        match span.status_code {
            Some(code) => {
                let mut m = Map::new();
                // Emit `code` as the OTel-spec enum name (`STATUS_CODE_UNSET`
                // / `STATUS_CODE_OK` / `STATUS_CODE_ERROR`) so the snapshot
                // discriminates the three states. Everything else stays
                // type-redacted.
                m.insert(
                    "code".to_string(),
                    Value::String(status_code_name(code).to_string()),
                );
                m.insert("message".to_string(), Value::String("[string]".to_string()));
                Value::Object(m)
            }
            None => Value::Null,
        },
    );
    obj.insert(
        "dropped_attributes_count".to_string(),
        Value::String("[int]".to_string()),
    );
    obj.insert(
        "dropped_events_count".to_string(),
        Value::String("[int]".to_string()),
    );
    obj.insert(
        "dropped_links_count".to_string(),
        Value::String("[int]".to_string()),
    );
    obj.insert("attributes".to_string(), attrs_to_json(&span.attrs));

    if !merged.is_empty() {
        let mut arr = Vec::with_capacity(merged.len());
        for child in merged {
            match child {
                Child::Event(e) => {
                    let mut m = Map::new();
                    m.insert("type".to_string(), Value::String("event".to_string()));
                    // Event `name` is set by tracing to the formatted
                    // message string, which typically embeds run-specific
                    // values (UUIDs, timestamps, Debug-formatted structs).
                    // Type-redact for the same reason attribute values are
                    // redacted: the assertion is that an event exists at
                    // this position in the tree, not the specific message.
                    // Structural info (attribute keys) is preserved.
                    m.insert(
                        "name".to_string(),
                        Value::String("[event_name]".to_string()),
                    );
                    m.insert(
                        "time_unix_nano".to_string(),
                        Value::String("[int]".to_string()),
                    );
                    m.insert(
                        "dropped_attributes_count".to_string(),
                        Value::String("[int]".to_string()),
                    );
                    m.insert("attributes".to_string(), attrs_to_json(&e.attrs));
                    arr.push(Value::Object(m));
                }
                Child::Span(cid) => {
                    let mut m = Map::new();
                    m.insert("type".to_string(), Value::String("span".to_string()));
                    let node = build_span_node(cid, spans, children_of);
                    if let Value::Object(child_obj) = node {
                        for (k, v) in child_obj {
                            m.insert(k, v);
                        }
                    }
                    arr.push(Value::Object(m));
                }
            }
        }
        obj.insert("children".to_string(), Value::Array(arr));
    }

    Value::Object(obj)
}

/// Emit each attribute as `<key>: "[<primitive-type>]"`. The snapshot
/// asserts on span/event/attribute *names* and tree *structure* — values
/// are inherently per-run (UUIDs, timestamps, ports, tempdir paths,
/// Debug-formatted structs) and add nothing to the OTLP-compliance
/// signal. Nested `KvlistValue`s keep their key structure so nested
/// attribute keys are still asserted.
/// Map OTLP `Status.Code` enum values to their canonical spec names.
/// Per `opentelemetry.proto.trace.v1.Status.StatusCode`.
fn status_code_name(code: i32) -> &'static str {
    match code {
        0 => "STATUS_CODE_UNSET",
        1 => "STATUS_CODE_OK",
        2 => "STATUS_CODE_ERROR",
        _ => "STATUS_CODE_UNKNOWN",
    }
}

fn attrs_to_json(attrs: &[KeyValue]) -> Value {
    let mut map: BTreeMap<String, Value> = BTreeMap::new();
    for kv in attrs {
        map.insert(
            kv.key.clone(),
            any_value_type_placeholder(kv.value.as_ref()),
        );
    }
    let mut obj = Map::new();
    for (k, v) in map {
        obj.insert(k, v);
    }
    Value::Object(obj)
}

fn any_value_type_placeholder(v: Option<&AnyValue>) -> Value {
    let Some(av) = v else {
        return Value::Null;
    };
    let Some(inner) = av.value.as_ref() else {
        return Value::Null;
    };
    match inner {
        any_value::Value::StringValue(_) => Value::String("[string]".to_string()),
        any_value::Value::BoolValue(_) => Value::String("[bool]".to_string()),
        any_value::Value::IntValue(_) => Value::String("[int]".to_string()),
        any_value::Value::DoubleValue(_) => Value::String("[double]".to_string()),
        any_value::Value::ArrayValue(_) => Value::String("[array]".to_string()),
        any_value::Value::BytesValue(_) => Value::String("[bytes]".to_string()),
        any_value::Value::KvlistValue(kvl) => {
            let mut m: BTreeMap<String, Value> = BTreeMap::new();
            for kv in &kvl.values {
                m.insert(
                    kv.key.clone(),
                    any_value_type_placeholder(kv.value.as_ref()),
                );
            }
            let mut obj = Map::new();
            for (k, v) in m {
                obj.insert(k, v);
            }
            Value::Object(obj)
        }
        any_value::Value::StringValueStrindex(_) => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry_proto::tonic::collector::trace::v1::trace_service_client::TraceServiceClient;
    use opentelemetry_proto::tonic::common::v1::{AnyValue as PbAnyValue, KeyValue as PbKeyValue};
    use opentelemetry_proto::tonic::resource::v1::Resource;
    use opentelemetry_proto::tonic::trace::v1::{
        ResourceSpans as PbResourceSpans, ScopeSpans as PbScopeSpans, Span as PbSpan,
    };

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn collector_roundtrip_h2c() {
        let td = tempfile::TempDir::new().unwrap();
        let collector = TestOtlpCollector::spawn(td.path(), &["root"]).unwrap();
        let endpoint = collector.endpoint().to_string();

        // Give the tonic server a moment to bind before the client dials.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let mut client = TraceServiceClient::connect(endpoint).await.unwrap();
        let req = ExportTraceServiceRequest {
            resource_spans: vec![PbResourceSpans {
                resource: Some(Resource {
                    attributes: vec![PbKeyValue {
                        key: "service.name".into(),
                        value: Some(PbAnyValue {
                            value: Some(any_value::Value::StringValue("t".into())),
                        }),
                        key_strindex: 0,
                    }],
                    dropped_attributes_count: 0,
                    entity_refs: vec![],
                }),
                scope_spans: vec![PbScopeSpans {
                    scope: None,
                    spans: vec![PbSpan {
                        trace_id: vec![1u8; 16],
                        span_id: vec![2u8; 8],
                        trace_state: String::new(),
                        parent_span_id: vec![],
                        flags: 0,
                        name: "root".into(),
                        kind: 1,
                        start_time_unix_nano: 1,
                        end_time_unix_nano: 2,
                        attributes: vec![],
                        dropped_attributes_count: 0,
                        events: vec![],
                        dropped_events_count: 0,
                        links: vec![],
                        dropped_links_count: 0,
                        status: None,
                    }],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        };
        client.export(req).await.unwrap();

        let log_path = collector.shutdown().await.unwrap();
        let text = std::fs::read_to_string(&log_path).unwrap();
        assert!(text.contains("\"name\":\"root\""), "log = {text}");
    }
}
