//! Behavioural tests for the emission macros.
//!
//! Everything is asserted through a subscriber rather than by inspecting
//! expansions: what matters is the event that reaches a layer — its target,
//! its level, its field names, their order, and the recorded value types.
//!
//! The subscribers here are shaped like the one `server` installs per
//! service: an optional metrics layer selected on the metrics target, a
//! global `EnvFilter` carrying `swgr::metrics=<level>`, and a log layer that
//! excludes the metrics target.

use std::fmt;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tracing::field::{Field, Visit};
use tracing::{Event, Level, Metadata, Subscriber};
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{EnvFilter, Layer};

// ---------------------------------------------------------------------------
// capture
// ---------------------------------------------------------------------------

/// A recorded field value, by the `Visit` method it arrived on.
#[derive(Clone, Debug, PartialEq)]
enum Val {
    U64(u64),
    I64(i64),
    F64(f64),
    Bool(bool),
    Str(String),
    /// `?expr`, and also `%expr` — `tracing` records a `DisplayValue` through
    /// `record_debug` with a `Debug` impl that defers to `Display`.
    Debug(String),
}

#[derive(Clone, Debug)]
struct Captured {
    target: String,
    level: Level,
    fields: Vec<(String, Val)>,
}

impl Captured {
    fn names(&self) -> Vec<&str> {
        self.fields.iter().map(|(k, _)| k.as_str()).collect()
    }

    fn get(&self, name: &str) -> Option<&Val> {
        self.fields.iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }
}

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<Captured>>>);

impl Capture {
    fn events(&self) -> Vec<Captured> {
        lock(&self.0).clone()
    }
}

struct CaptureVisitor<'a>(&'a mut Vec<(String, Val)>);

impl Visit for CaptureVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.0
            .push((field.name().to_owned(), Val::Debug(format!("{value:?}"))));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.push((field.name().to_owned(), Val::U64(value)));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.push((field.name().to_owned(), Val::I64(value)));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.0.push((field.name().to_owned(), Val::F64(value)));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.push((field.name().to_owned(), Val::Bool(value)));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0
            .push((field.name().to_owned(), Val::Str(value.to_owned())));
    }
}

impl<S: Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = Vec::new();
        event.record(&mut CaptureVisitor(&mut fields));
        let meta = event.metadata();
        lock(&self.0).push(Captured {
            target: meta.target().to_owned(),
            level: *meta.level(),
            fields,
        });
    }
}

/// An in-memory `MakeWriter`, for asserting on what the log layer writes.
#[derive(Clone, Default)]
struct LogBuffer(Arc<Mutex<Vec<u8>>>);

impl LogBuffer {
    fn contents(&self) -> String {
        String::from_utf8_lossy(&lock(&self.0)).into_owned()
    }
}

impl io::Write for LogBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        lock(&self.0).extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogBuffer {
    type Writer = LogBuffer;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// A poisoned lock in a test is not interesting: the panic that poisoned it
/// is what the runner reports.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ---------------------------------------------------------------------------
// subscribers
// ---------------------------------------------------------------------------

/// Reproduces `tracing_opentelemetry::MetricsFilter`, the per-layer filter
/// `MetricsLayer` carries internally. Kept here so this crate need not depend
/// on `tracing-opentelemetry`; `server` exercises the real one.
fn is_metrics_event(meta: &Metadata<'_>) -> bool {
    meta.is_event()
        && meta.fields().iter().any(|field| {
            let name = field.name();
            name.starts_with("monotonic_counter.")
                || name.starts_with("counter.")
                || name.starts_with("histogram.")
                || name.starts_with("gauge.")
        })
}

/// `server`'s `metrics_env_filter`: `RUST_LOG` plus a directive pinning the
/// metrics target to the configured level.
fn env_filter(rust_log: &str, metrics_level: &str) -> EnvFilter {
    let target = switchgear_metrics::metrics_target();
    let directive = format!("{target}={metrics_level}");
    EnvFilter::new(rust_log).add_directive(
        directive
            .parse()
            .unwrap_or_else(|e| panic!("parsing {directive}: {e}")),
    )
}

fn metrics_layer<S>(capture: Option<Capture>) -> impl Layer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let target = switchgear_metrics::metrics_target();
    capture.with_filter(filter_fn(move |m| {
        m.target() == target && is_metrics_event(m)
    }))
}

/// Metrics layer present, everything recorded: the shape most of these tests
/// want.
fn recording<T>(f: impl FnOnce() -> T) -> (T, Vec<Captured>) {
    with_metrics("trace", "info", f)
}

fn with_metrics<T>(
    metrics_level: &str,
    rust_log: &str,
    f: impl FnOnce() -> T,
) -> (T, Vec<Captured>) {
    let capture = Capture::default();
    let subscriber = tracing_subscriber::registry()
        .with(metrics_layer(Some(capture.clone())))
        .with(env_filter(rust_log, metrics_level));
    let out = tracing::subscriber::with_default(subscriber, f);
    (out, capture.events())
}

fn only(events: Vec<Captured>) -> Captured {
    assert_eq!(events.len(), 1, "expected exactly one event: {events:?}");
    events
        .into_iter()
        .next()
        .unwrap_or_else(|| unreachable!("length asserted"))
}

// ---------------------------------------------------------------------------
// the four macros
// ---------------------------------------------------------------------------

#[test]
fn monotonic_counter_emits_a_prefixed_field_on_the_metrics_target() {
    let (_, events) = recording(|| {
        switchgear_metrics::monotonic_counter!("swgr_x_total", 7u64, "outcome" => "success");
    });

    let event = only(events);
    assert_eq!(event.target, "swgr::metrics");
    assert_eq!(event.target, switchgear_metrics::metrics_target());
    assert_eq!(event.level, Level::INFO);
    assert_eq!(event.names(), ["outcome", "monotonic_counter.swgr_x_total"]);
    assert_eq!(
        event.get("monotonic_counter.swgr_x_total"),
        Some(&Val::U64(7))
    );
}

#[test]
fn counter_emits_a_prefixed_field_on_the_metrics_target() {
    let (_, events) = recording(|| {
        switchgear_metrics::counter!("swgr_x_inflight", 7i64, "outcome" => "success");
    });

    let event = only(events);
    assert_eq!(event.target, "swgr::metrics");
    assert_eq!(event.level, Level::INFO);
    assert_eq!(event.names(), ["outcome", "counter.swgr_x_inflight"]);
    assert_eq!(event.get("counter.swgr_x_inflight"), Some(&Val::I64(7)));
}

#[test]
fn histogram_emits_a_prefixed_field_on_the_metrics_target() {
    let (_, events) = recording(|| {
        switchgear_metrics::histogram!("swgr_x_ms", 7u64, "outcome" => "success");
    });

    let event = only(events);
    assert_eq!(event.target, "swgr::metrics");
    assert_eq!(event.level, Level::INFO);
    assert_eq!(event.names(), ["outcome", "histogram.swgr_x_ms"]);
    assert_eq!(event.get("histogram.swgr_x_ms"), Some(&Val::U64(7)));
}

#[test]
fn gauge_emits_a_prefixed_field_on_the_metrics_target() {
    let (_, events) = recording(|| {
        switchgear_metrics::gauge!("swgr_x_open", 7u64, "outcome" => "success");
    });

    let event = only(events);
    assert_eq!(event.target, "swgr::metrics");
    assert_eq!(event.level, Level::INFO);
    assert_eq!(event.names(), ["outcome", "gauge.swgr_x_open"]);
    assert_eq!(event.get("gauge.swgr_x_open"), Some(&Val::U64(7)));
}

#[test]
fn the_metric_field_comes_after_every_label() {
    let (_, events) = recording(|| {
        switchgear_metrics::histogram!(
            "swgr_x_ms",
            1u64,
            "a" => "1",
            "b" => "2",
            "c" => "3",
        );
    });

    assert_eq!(only(events).names(), ["a", "b", "c", "histogram.swgr_x_ms"]);
}

// ---------------------------------------------------------------------------
// label forms
// ---------------------------------------------------------------------------

#[test]
fn no_label_form_emits_only_the_metric_field() {
    let (_, events) = recording(|| {
        switchgear_metrics::counter!("swgr_x_inflight", 1i64);
    });

    assert_eq!(only(events).names(), ["counter.swgr_x_inflight"]);
}

#[test]
fn a_trailing_comma_is_accepted_with_and_without_labels() {
    let (_, events) = recording(|| {
        switchgear_metrics::counter!("swgr_a", 1i64,);
        switchgear_metrics::counter!("swgr_b", 1i64, "k" => "v",);
    });

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].names(), ["counter.swgr_a"]);
    assert_eq!(events[1].names(), ["k", "counter.swgr_b"]);
}

#[test]
fn label_forms_mix_in_one_call() {
    const BACKEND: &str = "ln.backend";

    let url = "https://node.example:9736";

    let (_, events) = recording(|| {
        switchgear_metrics::gauge!(
            "swgr_x_open",
            1u64,
            "outcome" => "success",       // string-literal key, string value
            BACKEND => "cln",             // const key
            "attempt" => 2u64,            // non-string value
            "url" => %url,                // Display
            "backend.id" => ?Some(9u32)   // Debug
        );
    });

    let event = only(events);
    assert_eq!(
        event.names(),
        [
            "outcome",
            "ln.backend",
            "attempt",
            "url",
            "backend.id",
            "gauge.swgr_x_open"
        ]
    );
    assert_eq!(event.get("outcome"), Some(&Val::Str("success".to_owned())));
    assert_eq!(event.get("ln.backend"), Some(&Val::Str("cln".to_owned())));
    assert_eq!(event.get("attempt"), Some(&Val::U64(2)));
    assert_eq!(event.get("url"), Some(&Val::Debug(url.to_owned())));
    assert_eq!(
        event.get("backend.id"),
        Some(&Val::Debug("Some(9)".to_owned()))
    );
}

#[test]
fn a_label_value_can_be_an_arbitrary_expression() {
    let response: Result<(), ()> = Ok(());

    let (_, events) = recording(|| {
        switchgear_metrics::histogram!(
            "swgr_x_ms",
            1u64,
            "outcome" => if response.is_ok() { "success" } else { "error" },
        );
    });

    assert_eq!(
        only(events).get("outcome"),
        Some(&Val::Str("success".to_owned()))
    );
}

// ---------------------------------------------------------------------------
// coercion
// ---------------------------------------------------------------------------

#[test]
fn narrow_unsigned_widens_to_u64() {
    let (_, events) = recording(|| {
        switchgear_metrics::histogram!("swgr_a", 5u32);
        switchgear_metrics::monotonic_counter!("swgr_b", 5usize);
        switchgear_metrics::gauge!("swgr_c", 5u8);
    });

    assert_eq!(events[0].get("histogram.swgr_a"), Some(&Val::U64(5)));
    assert_eq!(
        events[1].get("monotonic_counter.swgr_b"),
        Some(&Val::U64(5))
    );
    assert_eq!(events[2].get("gauge.swgr_c"), Some(&Val::U64(5)));
}

#[test]
fn narrow_signed_widens_to_i64_and_keeps_its_sign() {
    let (_, events) = recording(|| {
        switchgear_metrics::counter!("swgr_a", -3i32);
        switchgear_metrics::counter!("swgr_b", 3u32);
        switchgear_metrics::gauge!("swgr_c", -3i16);
    });

    assert_eq!(events[0].get("counter.swgr_a"), Some(&Val::I64(-3)));
    assert_eq!(events[1].get("counter.swgr_b"), Some(&Val::I64(3)));
    assert_eq!(events[2].get("gauge.swgr_c"), Some(&Val::I64(-3)));
}

#[test]
fn f32_widens_to_f64() {
    let (_, events) = recording(|| {
        switchgear_metrics::histogram!("swgr_a", 1.5f32);
        switchgear_metrics::counter!("swgr_b", -1.5f32);
    });

    assert_eq!(events[0].get("histogram.swgr_a"), Some(&Val::F64(1.5)));
    assert_eq!(events[1].get("counter.swgr_b"), Some(&Val::F64(-1.5)));
}

#[test]
fn duration_records_as_milliseconds_in_f64() {
    let elapsed = Duration::from_micros(1_234_567);
    let hand_written = elapsed.as_secs_f64() * 1_000.0;

    let (_, events) = recording(|| {
        switchgear_metrics::histogram!("swgr_a_ms", elapsed);
        switchgear_metrics::gauge!("swgr_b_ms", elapsed);
        switchgear_metrics::monotonic_counter!("swgr_c_ms", elapsed);
    });

    assert_eq!(
        events[0].get("histogram.swgr_a_ms"),
        Some(&Val::F64(hand_written))
    );
    assert_eq!(
        events[1].get("gauge.swgr_b_ms"),
        Some(&Val::F64(hand_written))
    );
    assert_eq!(
        events[2].get("monotonic_counter.swgr_c_ms"),
        Some(&Val::F64(hand_written))
    );
}

// ---------------------------------------------------------------------------
// levels
// ---------------------------------------------------------------------------

#[test]
fn level_defaults_to_info() {
    let (_, events) = with_metrics("info", "info", || {
        switchgear_metrics::histogram!("swgr_default_level_ms", 1u64);
    });

    assert_eq!(only(events).level, Level::INFO);
}

#[test]
fn an_explicit_level_is_carried_onto_the_event() {
    let (_, events) = with_metrics("trace", "info", || {
        switchgear_metrics::histogram!(level: Level::WARN, "swgr_warn_ms", 1u64);
    });

    assert_eq!(only(events).level, Level::WARN);
}

#[test]
fn a_debug_metric_is_dropped_under_an_info_directive() {
    let (_, events) = with_metrics("info", "info", || {
        switchgear_metrics::histogram!(level: Level::DEBUG, "swgr_detail_a_ms", 1u64);
    });

    assert!(events.is_empty(), "recorded under =info: {events:?}");
}

#[test]
fn a_debug_metric_is_recorded_under_a_debug_directive() {
    let (_, events) = with_metrics("debug", "info", || {
        switchgear_metrics::histogram!(level: Level::DEBUG, "swgr_detail_b_ms", 1u64);
    });

    let event = only(events);
    assert_eq!(event.level, Level::DEBUG);
    assert_eq!(event.names(), ["histogram.swgr_detail_b_ms"]);
}

#[test]
fn an_info_metric_survives_a_debug_directive() {
    let (_, events) = with_metrics("debug", "info", || {
        switchgear_metrics::histogram!("swgr_detail_c_ms", 1u64);
    });

    assert_eq!(only(events).level, Level::INFO);
}

/// A dropped metric costs the filter check and nothing else: the value and the
/// label expressions sit inside `event!`'s enabled branch.
///
/// This holds while a dispatcher has been set — which `swgr service` always
/// does. `tracing`'s `log` feature is on in this workspace (sea-orm, sqlx,
/// tower pull it in), and its fallback path *does* reach the value set, but
/// only when `dispatcher::has_been_set()` is false.
#[test]
fn a_dropped_metric_evaluates_neither_its_value_nor_its_labels() {
    static VALUES: AtomicUsize = AtomicUsize::new(0);
    static LABELS: AtomicUsize = AtomicUsize::new(0);

    fn elapsed_ms() -> u64 {
        VALUES.fetch_add(1, Ordering::Relaxed);
        7
    }

    fn backend() -> &'static str {
        LABELS.fetch_add(1, Ordering::Relaxed);
        "cln"
    }

    fn emit() {
        switchgear_metrics::histogram!("swgr_lazy_ms", elapsed_ms(), "ln.backend" => backend());
    }

    let (_, events) = with_metrics("off", "info", emit);
    assert!(events.is_empty(), "recorded under =off: {events:?}");
    assert_eq!(VALUES.load(Ordering::Relaxed), 0, "value expression ran");
    assert_eq!(LABELS.load(Ordering::Relaxed), 0, "label expression ran");

    let (_, events) = with_metrics("info", "info", emit);
    assert_eq!(
        only(events).get("histogram.swgr_lazy_ms"),
        Some(&Val::U64(7))
    );
    assert_eq!(
        VALUES.load(Ordering::Relaxed),
        1,
        "value expression skipped"
    );
    assert_eq!(
        LABELS.load(Ordering::Relaxed),
        1,
        "label expression skipped"
    );
}

// ---------------------------------------------------------------------------
// the log layer
// ---------------------------------------------------------------------------

#[test]
fn the_log_layer_writes_no_metric_record() {
    for metrics_layer_present in [true, false] {
        let capture = Capture::default();
        let logs = LogBuffer::default();
        let target = switchgear_metrics::metrics_target();

        let subscriber = tracing_subscriber::registry()
            .with(metrics_layer(
                metrics_layer_present.then(|| capture.clone()),
            ))
            .with(env_filter(
                "info",
                if metrics_layer_present { "info" } else { "off" },
            ))
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(logs.clone())
                    .with_ansi(false)
                    .with_filter(filter_fn(move |m| m.target() != target)),
            );

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(answer = 42, "an ordinary log line");
            switchgear_metrics::histogram!(
                "swgr_log_layer_ms",
                1u64,
                "ln.backend" => "cln",
            );
        });

        let written = logs.contents();
        let case = format!("metrics={metrics_layer_present}");
        assert!(
            written.contains("an ordinary log line"),
            "log line missing, {case}: {written}"
        );
        assert!(
            !written.contains("swgr::metrics"),
            "metrics target in log output, {case}: {written}"
        );
        assert!(
            !written.contains("histogram."),
            "metric field in log output, {case}: {written}"
        );
        assert!(
            !written.contains("swgr_log_layer_ms"),
            "metric name in log output, {case}: {written}"
        );
        assert!(
            !written.contains("off"),
            "directive level in log output, {case}: {written}"
        );
    }
}

// ---------------------------------------------------------------------------
// target
// ---------------------------------------------------------------------------

#[test]
fn metrics_target_is_constant() {
    assert_eq!(switchgear_metrics::metrics_target(), "swgr::metrics");
    assert_eq!(
        switchgear_metrics::metrics_target(),
        switchgear_metrics::metrics_target()
    );
}
