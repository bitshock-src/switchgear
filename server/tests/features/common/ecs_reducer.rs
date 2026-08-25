//! ECS-compliance stderr reducer for `insta` snapshot tests.
//!
//! The child `swgr` process emits ECS-JSON records to `stderr` via
//! `EcsFormatter` (see
//! `server/src/di/inject/injectors/service/tracing.rs`). The per-server
//! `stderr_buffer` captures each line. This reducer takes the raw
//! `Vec<String>` buffer, filters it to a caller-declared set of interesting
//! ECS records, redacts every field value to a `[<type>]` placeholder
//! (mirroring `otlp_collector::any_value_type_placeholder`), and sorts the
//! result deterministically so `insta` diffs are stable across runs.
//!
//! The output is the bidirectional assertion "exactly these ECS fields on
//! exactly these records": the top-level list is exhaustive within the
//! filter, and per-record keys are sorted lexicographically. Any new/removed
//! field or record shows up in the `insta` diff.

use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fmt;

/// Filter over ECS records emitted to stderr. Narrows a raw stderr buffer
/// to the records the snapshot audits — everything else is dropped before
/// redaction. Combines conjunctively across dimensions and disjunctively
/// within a dimension (same semantic as `EcsRequestFilter`, extended to
/// slices).
///
/// Optional dimensions apply only to records that carry the discriminator
/// field. A record without `url.path` passes the `url_path_prefixes`
/// check; a record without `error.type` passes the `error_types` check.
/// Records are matched by whichever dimensions they do carry — the
/// `service_names` / `levels` gates apply universally.
pub struct EcsFilter<'a> {
    /// Match `service.name` exactly. Both the value-side (`"swgr.lnurl"`,
    /// `"swgr.discovery"`, `"swgr.offer"`) and the bare `"swgr"`
    /// pre-service-subscriber lines are valid inputs; the filter is
    /// exact-match to keep the snapshot small.
    pub service_names: &'a [&'a str],
    /// Optional: restrict to a subset of levels
    /// (`Some(&["INFO"])` for access-only, etc.). `None` = all.
    pub levels: Option<&'a [&'a str]>,
    /// Optional: keep only lines whose `url.path` starts with one of
    /// these prefixes. Applied only to lines that have `url.path`
    /// (access logs). Error logs without `url.path` pass through.
    pub url_path_prefixes: Option<&'a [&'a str]>,
    /// Optional: keep only error-triple lines whose `error.type`
    /// matches one of these values (Debug type-name from
    /// `std::any::type_name`). Applied only to lines that have
    /// `error.type`. Access logs without `error.type` pass through.
    pub error_types: Option<&'a [&'a str]>,
}

pub struct EcsReducer;

impl EcsReducer {
    /// Filter stderr lines to ECS JSON records, drop noise, redact values,
    /// sort deterministically. Malformed lines and non-ECS JSON are dropped
    /// silently — bad JSON is not part of the ECS wire.
    pub fn reduce(stderr_lines: &[String], filter: &EcsFilter<'_>) -> Vec<Value> {
        let mut records: Vec<Value> = stderr_lines
            .iter()
            .filter_map(|line| parse_ecs_object(line))
            .filter(|v| matches(v, filter))
            .map(|v| redact_object(&v))
            .collect();

        records.sort_by_key(sort_key);
        records
    }

    /// Return the wire-order top-level key sequence for each ECS record
    /// matching `filter`. Preserves insertion order (unlike `reduce`, which
    /// sorts) so tests can assert the ECS-logging spec's field-ordering
    /// requirement:
    ///
    /// > The ordering of the next three keys must be respected in every
    /// > ecs-logging library: `@timestamp`, `log.level`, `message` (or
    /// > absent). With the fourth key, `ecs.version`, in the ND-JSON
    /// > output, we define the minimum viable product (MVP) for a log line.
    /// >
    /// > — https://github.com/elastic/ecs-logging/blob/main/spec/README.md
    ///
    /// Reads each line with a streaming `serde::Deserializer` visitor that
    /// records keys in the order the wire produced them, independent of
    /// `serde_json::Map`'s default `BTreeMap` reorder. Lines that do not
    /// parse as JSON objects or carry `ecs.version` are dropped; filter
    /// matching then uses `reduce`'s parse path so the two methods agree
    /// on record identity.
    pub fn key_orders(stderr_lines: &[String], filter: &EcsFilter<'_>) -> Vec<Vec<String>> {
        stderr_lines
            .iter()
            .filter_map(|line| {
                let parsed = parse_ecs_object(line)?;
                if !matches(&parsed, filter) {
                    return None;
                }
                extract_key_order(line)
            })
            .collect()
    }
}

/// Wire-order top-level key extraction. Uses a streaming
/// `serde::Deserializer` visit — `serde_json::Value` deserializes into a
/// `BTreeMap` by default and would lose insertion order otherwise.
fn extract_key_order(line: &str) -> Option<Vec<String>> {
    let mut de = serde_json::Deserializer::from_str(line);
    serde::Deserializer::deserialize_map(&mut de, TopLevelKeyOrderVisitor).ok()
}

struct TopLevelKeyOrderVisitor;

impl<'de> Visitor<'de> for TopLevelKeyOrderVisitor {
    type Value = Vec<String>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Vec<String>, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut keys = Vec::new();
        while let Some(key) = map.next_key::<String>()? {
            keys.push(key);
            let _: IgnoredAny = map.next_value()?;
        }
        Ok(keys)
    }
}

/// Parse a stderr line as an ECS JSON record — must be a JSON object and
/// carry `ecs.version` (the discriminator against plain-text panics, OTel
/// SDK log spam, `tokio-listener` bind messages, etc.).
fn parse_ecs_object(line: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(line).ok()?;
    if !v.is_object() {
        return None;
    }
    v.get("ecs.version").and_then(|x| x.as_str())?;
    Some(v)
}

fn matches(v: &Value, filter: &EcsFilter<'_>) -> bool {
    let service = v.get("service.name").and_then(|x| x.as_str());
    let Some(service) = service else {
        return false;
    };
    if !filter.service_names.contains(&service) {
        return false;
    }

    if let Some(levels) = filter.levels {
        let level = v.get("log.level").and_then(|x| x.as_str()).unwrap_or("");
        if !levels.contains(&level) {
            return false;
        }
    }

    if let Some(prefixes) = filter.url_path_prefixes
        && let Some(path) = v.get("url.path").and_then(|x| x.as_str())
        && !prefixes.iter().any(|p| path.starts_with(p))
    {
        return false;
    }

    if let Some(types) = filter.error_types
        && let Some(t) = v.get("error.type").and_then(|x| x.as_str())
        && !types.contains(&t)
    {
        return false;
    }

    true
}

/// Type-redact a JSON value. Object keys are preserved verbatim so the
/// snapshot asserts field names; leaf values become `[<type>]` placeholders
/// so run-specific data (timestamps, IDs, tempdir paths) doesn't churn.
///
/// Two field-level carve-outs are handled by `redact_object` on the top-level
/// record: `log.level` and `service.name` stay literal so the snapshot
/// reader can discriminate INFO/WARN/ERROR and lnurl/discovery/offer at a
/// glance.
fn redact(v: &Value) -> Value {
    match v {
        Value::Null => Value::Null,
        Value::Bool(_) => Value::String("[bool]".into()),
        Value::Number(n) => Value::String(
            if n.is_i64() || n.is_u64() {
                "[int]"
            } else {
                "[float]"
            }
            .into(),
        ),
        Value::String(_) => Value::String("[string]".into()),
        Value::Array(arr) => Value::Array(arr.iter().map(redact).collect()),
        Value::Object(map) => {
            let mut sorted: BTreeMap<String, Value> = BTreeMap::new();
            for (k, val) in map {
                sorted.insert(k.clone(), redact(val));
            }
            let mut out = Map::new();
            for (k, val) in sorted {
                out.insert(k, val);
            }
            Value::Object(out)
        }
    }
}

/// Redact a top-level ECS record. Keeps `log.level` and `service.name`
/// literal (snapshot-readability + sort-key discriminators); everything
/// else runs through `redact`.
fn redact_object(v: &Value) -> Value {
    let Value::Object(map) = v else {
        return redact(v);
    };
    let mut sorted: BTreeMap<String, Value> = BTreeMap::new();
    for (k, val) in map {
        let redacted = match k.as_str() {
            "log.level" | "service.name" => val.clone(),
            _ => redact(val),
        };
        sorted.insert(k.clone(), redacted);
    }
    let mut out = Map::new();
    for (k, val) in sorted {
        out.insert(k, val);
    }
    Value::Object(out)
}

/// Total sort key for an already-redacted record. Groups by
/// (service.name, log.level, http.request.method, error.type) then falls
/// back to the full serialized JSON for a stable tie-break.
fn sort_key(v: &Value) -> (String, String, String, String, String) {
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    (
        s("service.name"),
        s("log.level"),
        s("http.request.method"),
        s("error.type"),
        serde_json::to_string(v).unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info_access_line() -> String {
        r#"{"@timestamp":"2024-01-01T00:00:00Z","log.level":"INFO","message":"request","ecs.version":"8.11.0","trace.id":"abc","span.id":"def","service.name":"swgr.lnurl","service.version":"0.0.1","event.module":"swgr::request","http.request.method":"GET","http.response.status_code":200,"http.version":"1.1","url.path":"/offers/default/x/invoice","url.query":"amount=100000","client.ip":"127.0.0.1","event.duration":123456}"#.to_string()
    }

    fn warn_error_line() -> String {
        r#"{"@timestamp":"2024-01-01T00:00:00Z","log.level":"WARN","message":"not found","ecs.version":"8.11.0","trace.id":"abc","span.id":"def","service.name":"swgr.lnurl","service.version":"0.0.1","event.module":"swgr","error.type":"switchgear_service::lnurl::pay::error::LnUrlPayServiceError","error.message":"not found","error.stack_trace":"chain","log.origin.file.name":"error.rs","log.origin.file.line":110,"http.response.status_code":404}"#.to_string()
    }

    fn plain_noise_line() -> String {
        "listening on 127.0.0.1:1234".to_string()
    }

    fn json_no_ecs_line() -> String {
        r#"{"level":"INFO","message":"other"}"#.to_string()
    }

    #[test]
    fn drops_non_json_and_non_ecs() {
        let lines = vec![plain_noise_line(), json_no_ecs_line(), info_access_line()];
        let filter = EcsFilter {
            service_names: &["swgr.lnurl"],
            levels: None,
            url_path_prefixes: None,
            error_types: None,
        };
        let out = EcsReducer::reduce(&lines, &filter);
        assert_eq!(out.len(), 1, "only the ECS line should survive");
    }

    #[test]
    fn keeps_literal_level_and_service_name() {
        let lines = vec![info_access_line()];
        let filter = EcsFilter {
            service_names: &["swgr.lnurl"],
            levels: Some(&["INFO"]),
            url_path_prefixes: None,
            error_types: None,
        };
        let out = EcsReducer::reduce(&lines, &filter);
        assert_eq!(out.len(), 1);
        let r = &out[0];
        assert_eq!(r["log.level"], "INFO");
        assert_eq!(r["service.name"], "swgr.lnurl");
        assert_eq!(r["@timestamp"], "[string]");
        assert_eq!(r["http.response.status_code"], "[int]");
        assert_eq!(r["url.path"], "[string]");
    }

    #[test]
    fn url_path_prefix_filter_applies_only_to_records_with_url_path() {
        let lines = vec![info_access_line(), warn_error_line()];
        let filter = EcsFilter {
            service_names: &["swgr.lnurl"],
            levels: Some(&["INFO", "WARN"]),
            url_path_prefixes: Some(&["/offers/default/"]),
            error_types: Some(&["switchgear_service::lnurl::pay::error::LnUrlPayServiceError"]),
        };
        let out = EcsReducer::reduce(&lines, &filter);
        // Access line matches url.path prefix; warn line has no url.path so
        // passes the url filter, and its error.type matches — both survive.
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn url_path_prefix_filter_drops_mismatched_access() {
        let lines = vec![info_access_line()];
        let filter = EcsFilter {
            service_names: &["swgr.lnurl"],
            levels: Some(&["INFO"]),
            url_path_prefixes: Some(&["/nope/"]),
            error_types: None,
        };
        let out = EcsReducer::reduce(&lines, &filter);
        assert_eq!(out.len(), 0);
    }

    #[test]
    fn key_orders_preserves_wire_order() {
        // Deliberately reorder keys away from the spec order in the input
        // to prove the method reads wire order, not any sorted form.
        let out_of_spec = r#"{"ecs.version":"8.11.0","log.level":"INFO","@timestamp":"t","message":"m","service.name":"swgr.lnurl"}"#.to_string();
        let filter = EcsFilter {
            service_names: &["swgr.lnurl"],
            levels: None,
            url_path_prefixes: None,
            error_types: None,
        };
        let orders = EcsReducer::key_orders(&[out_of_spec], &filter);
        assert_eq!(orders.len(), 1);
        assert_eq!(
            orders[0],
            vec![
                "ecs.version".to_string(),
                "log.level".to_string(),
                "@timestamp".to_string(),
                "message".to_string(),
                "service.name".to_string(),
            ]
        );
    }

    #[test]
    fn key_orders_returns_spec_order_for_correctly_ordered_line() {
        let line = info_access_line();
        let filter = EcsFilter {
            service_names: &["swgr.lnurl"],
            levels: Some(&["INFO"]),
            url_path_prefixes: None,
            error_types: None,
        };
        let orders = EcsReducer::key_orders(&[line], &filter);
        assert_eq!(orders.len(), 1);
        // The fixture is in spec order.
        assert_eq!(
            &orders[0][..4],
            &["@timestamp", "log.level", "message", "ecs.version"]
        );
    }

    #[test]
    fn key_orders_drops_non_ecs_lines() {
        let lines = vec![plain_noise_line(), json_no_ecs_line(), info_access_line()];
        let filter = EcsFilter {
            service_names: &["swgr.lnurl"],
            levels: None,
            url_path_prefixes: None,
            error_types: None,
        };
        let orders = EcsReducer::key_orders(&lines, &filter);
        assert_eq!(orders.len(), 1);
    }

    #[test]
    fn sort_is_stable_across_input_order() {
        let a = info_access_line();
        let b = warn_error_line();
        let filter = EcsFilter {
            service_names: &["swgr.lnurl"],
            levels: Some(&["INFO", "WARN"]),
            url_path_prefixes: None,
            error_types: None,
        };
        let out_ab = EcsReducer::reduce(&[a.clone(), b.clone()], &filter);
        let out_ba = EcsReducer::reduce(&[b, a], &filter);
        assert_eq!(out_ab, out_ba);
    }
}
