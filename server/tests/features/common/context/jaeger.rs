use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};
use tokio::time::sleep;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(500);
const DEFAULT_LOOKBACK: Duration = Duration::from_secs(300);
const DEFAULT_LIMIT: usize = 50;

#[derive(Clone, Debug)]
pub struct JaegerClient {
    base_url: String,
    timeout: Duration,
    poll_interval: Duration,
}

impl JaegerClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            timeout: DEFAULT_TIMEOUT,
            poll_interval: DEFAULT_POLL_INTERVAL,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_poll_interval(mut self, poll_interval: Duration) -> Self {
        self.poll_interval = poll_interval;
        self
    }

    /// Poll Jaeger until at least one trace matching `query` and (optionally)
    /// `predicate` is found. Errors if the timeout elapses first.
    pub async fn wait_for_trace<F>(&self, query: &TraceQuery, predicate: F) -> Result<JaegerTrace>
    where
        F: Fn(&JaegerTrace) -> bool,
    {
        let url = query.build_url(&self.base_url);
        let deadline = Instant::now() + self.timeout;
        let mut last_error: String;
        loop {
            match Self::fetch(&url).await {
                Ok(traces) => {
                    if let Some(t) = traces.into_iter().find(&predicate) {
                        return Ok(t);
                    }
                    last_error = format!("no trace matched predicate at {url} (query={query:?})");
                }
                Err(e) => last_error = format!("{e:#}"),
            }
            if Instant::now() >= deadline {
                return Err(anyhow!(
                    "Jaeger wait_for_trace timeout after {:?} for {query:?}: {last_error}",
                    self.timeout
                ));
            }
            sleep(self.poll_interval).await;
        }
    }

    /// Poll until any span across returned traces matches `predicate`. Returns
    /// the parent trace along with the matched span index for convenience.
    pub async fn wait_for_span<F>(&self, query: &TraceQuery, predicate: F) -> Result<MatchedSpan>
    where
        F: Fn(&JaegerSpan) -> bool,
    {
        let trace = self
            .wait_for_trace(query, |t| t.spans.iter().any(&predicate))
            .await?;
        let (idx, _) = trace
            .spans
            .iter()
            .enumerate()
            .find(|(_, s)| predicate(s))
            .ok_or_else(|| anyhow!("matched span disappeared from trace"))?;
        Ok(MatchedSpan {
            trace,
            span_index: idx,
        })
    }

    async fn fetch(url: &str) -> Result<Vec<JaegerTrace>> {
        let resp = reqwest::get(url)
            .await
            .with_context(|| format!("GET {url}"))?;
        let status = resp.status();
        let body = resp
            .bytes()
            .await
            .with_context(|| format!("reading body from {url}"))?;
        if !status.is_success() {
            return Err(anyhow!(
                "Jaeger returned status={status} body={}",
                String::from_utf8_lossy(&body)
            ));
        }
        let json: Value = serde_json::from_slice(&body).with_context(|| {
            format!(
                "parsing Jaeger JSON from {url}: {}",
                String::from_utf8_lossy(&body)
            )
        })?;
        let data = json
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(data.into_iter().map(JaegerTrace::from_json).collect())
    }
}

/// A Jaeger `/api/traces` query. Server-side filters map to Jaeger's REST
/// query parameters; use `predicate` on the client for anything more exotic.
#[derive(Clone, Debug)]
pub struct TraceQuery {
    service: String,
    operation: Option<String>,
    tags: BTreeMap<String, String>,
    lookback: Duration,
    limit: usize,
}

impl TraceQuery {
    pub fn service(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            operation: None,
            tags: BTreeMap::new(),
            lookback: DEFAULT_LOOKBACK,
            limit: DEFAULT_LIMIT,
        }
    }

    pub fn operation(mut self, op: impl Into<String>) -> Self {
        self.operation = Some(op.into());
        self
    }

    pub fn tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }

    pub fn lookback(mut self, lookback: Duration) -> Self {
        self.lookback = lookback;
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    fn build_url(&self, base: &str) -> String {
        let lookback_us = self.lookback.as_micros();
        let mut parts = vec![
            format!("service={}", urlencode(&self.service)),
            format!("limit={}", self.limit),
            format!("lookback={lookback_us}us"),
        ];
        if let Some(op) = &self.operation {
            parts.push(format!("operation={}", urlencode(op)));
        }
        if !self.tags.is_empty() {
            let json = serde_json::to_string(&self.tags).expect("tag map serialisable");
            parts.push(format!("tags={}", urlencode(&json)));
        }
        format!("{base}/api/traces?{}", parts.join("&"))
    }
}

fn urlencode(s: &str) -> String {
    // Minimal query-safe encoding — Jaeger accepts standard percent-encoding.
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[derive(Clone, Debug)]
pub struct JaegerTrace {
    pub trace_id: String,
    pub spans: Vec<JaegerSpan>,
    pub processes: HashMap<String, JaegerProcess>,
}

#[derive(Clone, Debug)]
pub struct JaegerProcess {
    pub service_name: String,
    pub tags: HashMap<String, String>,
}

impl JaegerTrace {
    fn from_json(v: Value) -> Self {
        let trace_id = v
            .get("traceID")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let spans = v
            .get("spans")
            .and_then(|s| s.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|s| JaegerSpan::from_json(s.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let processes = v
            .get("processes")
            .and_then(|p| p.as_object())
            .map(|obj| {
                obj.iter()
                    .map(|(pid, proc)| {
                        let service_name = proc
                            .get("serviceName")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string();
                        let tags = kv_array_to_map(proc.get("tags"));
                        (pid.clone(), JaegerProcess { service_name, tags })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            trace_id,
            spans,
            processes,
        }
    }

    pub fn spans_by_operation<'a>(&'a self, name: &str) -> Vec<&'a JaegerSpan> {
        self.spans
            .iter()
            .filter(|s| s.operation_name == name)
            .collect()
    }

    pub fn span_by_operation<'a>(&'a self, name: &str) -> Option<&'a JaegerSpan> {
        self.spans.iter().find(|s| s.operation_name == name)
    }

    /// Root span of the trace — the one without any CHILD_OF/FOLLOWS_FROM
    /// parent reference. `OtelAxumLayer` opens exactly one such span per
    /// request (`"{METHOD} {route}"`).
    pub fn root_span(&self) -> Option<&JaegerSpan> {
        self.spans.iter().find(|s| s.references.is_empty())
    }

    /// Resolve a span's `service.name` from the trace's `processes` map (which
    /// mirrors the OTLP Resource on the sending side).
    fn span_service_name(&self, span: &JaegerSpan) -> Option<&str> {
        self.processes
            .get(span.process_id.as_str())
            .map(|p| p.service_name.as_str())
    }

    /// Search every span in the trace for `service.name`, checking the root
    /// span first. `service.name` is sourced from the OTLP Resource (Jaeger's
    /// per-trace `processes` map), not from span attributes or events.
    pub fn find_service_name(&self) -> Option<&str> {
        self.root_span()
            .and_then(|s| self.span_service_name(s))
            .or_else(|| self.spans.iter().find_map(|s| self.span_service_name(s)))
    }

    /// Search every span in the trace for `error.message`, root first.
    pub fn find_error_message(&self) -> Option<&str> {
        self.root_span()
            .and_then(|s| s.error_message())
            .or_else(|| self.spans.iter().find_map(|s| s.error_message()))
    }
}

#[derive(Clone, Debug)]
pub struct JaegerSpan {
    pub trace_id: String,
    pub span_id: String,
    pub operation_name: String,
    pub duration_us: u64,
    pub tags: HashMap<String, String>,
    pub logs: Vec<HashMap<String, String>>,
    pub references: Vec<SpanReference>,
    pub process_id: String,
}

#[derive(Clone, Debug)]
pub struct SpanReference {
    pub ref_type: String,
    pub trace_id: String,
    pub span_id: String,
}

impl JaegerSpan {
    fn from_json(v: Value) -> Self {
        let trace_id = v
            .get("traceID")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let span_id = v
            .get("spanID")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let operation_name = v
            .get("operationName")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let duration_us = v.get("duration").and_then(|v| v.as_u64()).unwrap_or(0);
        let tags = kv_array_to_map(v.get("tags"));
        let logs = v
            .get("logs")
            .and_then(|l| l.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|log| kv_array_to_map(log.get("fields")))
                    .collect()
            })
            .unwrap_or_default();
        let references = v
            .get("references")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|r| SpanReference {
                        ref_type: r
                            .get("refType")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        trace_id: r
                            .get("traceID")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        span_id: r
                            .get("spanID")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let process_id = v
            .get("processID")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        Self {
            trace_id,
            span_id,
            operation_name,
            duration_us,
            tags,
            logs,
            references,
            process_id,
        }
    }

    pub fn tag(&self, key: &str) -> Option<&str> {
        self.tags.get(key).map(|s| s.as_str())
    }

    /// Look for a field with `key` across all span logs, returning the first match.
    pub fn log_field(&self, key: &str) -> Option<&str> {
        self.logs
            .iter()
            .find_map(|log| log.get(key).map(|s| s.as_str()))
    }

    /// Count span events (Jaeger "logs") whose OTel event name matches `name`.
    /// Jaeger encodes the OTel span-event name in the `event` field.
    pub fn count_logs_named(&self, name: &str) -> usize {
        self.logs
            .iter()
            .filter(|log| log.get("event").map(|s| s.as_str()) == Some(name))
            .count()
    }

    pub fn http_status(&self) -> Option<u64> {
        self.tag("http.response.status_code")
            .or_else(|| self.tag("http.status_code"))
            .or_else(|| self.log_field("http.response.status_code"))
            .and_then(|s| s.parse().ok())
    }

    pub fn error_message(&self) -> Option<&str> {
        self.log_field("error.message")
    }

    pub fn url_path(&self) -> Option<&str> {
        self.tag("url.path").or_else(|| self.log_field("url.path"))
    }

    pub fn http_method(&self) -> Option<&str> {
        self.tag("http.request.method")
            .or_else(|| self.log_field("http.request.method"))
    }

    pub fn parent_span_id(&self) -> Option<&str> {
        self.references
            .iter()
            .find(|r| r.ref_type == "CHILD_OF")
            .map(|r| r.span_id.as_str())
    }
}

fn kv_array_to_map(v: Option<&Value>) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(arr) = v.and_then(|v| v.as_array()) else {
        return out;
    };
    for kv in arr {
        let (Some(key), value) = (kv.get("key").and_then(|k| k.as_str()), kv.get("value")) else {
            continue;
        };
        let value_str = match value {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Bool(b)) => b.to_string(),
            Some(Value::Number(n)) => n.to_string(),
            Some(Value::Null) | None => String::new(),
            Some(other) => other.to_string(),
        };
        out.insert(key.to_string(), value_str);
    }
    out
}

/// Result returned by `JaegerClient::wait_for_span` — the whole trace plus the
/// index of the matched span, so callers can inspect siblings/parents too.
#[derive(Clone, Debug)]
pub struct MatchedSpan {
    pub trace: JaegerTrace,
    pub span_index: usize,
}

impl MatchedSpan {
    pub fn span(&self) -> &JaegerSpan {
        &self.trace.spans[self.span_index]
    }
}
