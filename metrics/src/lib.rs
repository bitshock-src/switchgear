//! Metric emission for the Switchgear workspace.
//!
//! Metrics are `tracing` events on a dedicated target, bridged to
//! OpenTelemetry instruments by `tracing-opentelemetry`'s `MetricsLayer`.
//!
//! ```
//! use std::time::Instant;
//!
//! let started = Instant::now();
//! switchgear_metrics::histogram!(
//!     "swgr_ln_grpc_invoice_request_ms",
//!     started.elapsed(),
//!     "outcome" => "success",
//!     "ln.backend" => "cln",
//! );
//! ```
//!
//! # Macros
//!
//! | Macro | Field prefix | Instrument |
//! |---|---|---|
//! | [`monotonic_counter!`] | `monotonic_counter.` | counter |
//! | [`counter!`] | `counter.` | up-down counter |
//! | [`histogram!`] | `histogram.` | histogram |
//! | [`gauge!`] | `gauge.` | gauge |
//!
//! All four share one shape:
//!
//! ```text
//! <kind>!( [level: <expr>,] <name:literal>, <value:expr> [, <key:expr> => [%|?] <value:expr>]* [,] )
//! ```
//!
//! * `level:` — optional named parameter, before the name. Defaults to
//!   `tracing::Level::INFO`. See [Levels](#levels).
//! * `<name>` — a string literal. The macro prepends the prefix, so
//!   `histogram!("swgr_x_ms", …)` emits the field `histogram.swgr_x_ms` and
//!   the instrument is named `swgr_x_ms`.
//! * `<value>` — constrained per kind. See [Value types](#value-types).
//! * Label keys — constant expressions: a string literal
//!   (`"ln.backend" => …`) or a `const` (`BACKEND => …`).
//! * Label values — anything implementing `tracing::Value`, or `%expr` for
//!   `Display` and `?expr` for `Debug`. The three forms mix freely in one
//!   call.
//! * Field order — labels first, metric field last.
//!
//! ```
//! const BACKEND: &str = "ln.backend";
//! # #[derive(Debug)] struct BackendId(u32);
//! # let backend_id = BackendId(7);
//! # let url = "https://node.example:9736";
//! switchgear_metrics::counter!(
//!     "swgr_x_inflight",
//!     1i64,
//!     BACKEND => "cln",            // const key
//!     "url" => %url,               // Display
//!     "backend.id" => ?backend_id, // Debug
//!     "attempt" => 2u64,           // non-string value
//! );
//! ```
//!
//! # Levels
//!
//! A metric's level is compared against the `EnvFilter` directive for the
//! metrics target. The threshold comes from service config, not `RUST_LOG`:
//!
//! ```yaml
//! otlp:
//!   metrics:
//!     temporality: cumulative
//!     level: debug          # trace|debug|info|warn|error|off; optional, defaults to info
//! ```
//!
//! ```
//! use tracing::Level;
//! # let started = std::time::Instant::now();
//! switchgear_metrics::histogram!(
//!     "swgr_ln_grpc_invoice_request_ms",
//!     started.elapsed(),
//!     "ln.backend" => "cln",
//! );
//!
//! switchgear_metrics::histogram!(
//!     level: Level::DEBUG,
//!     "swgr_ln_grpc_invoice_detail_ms",
//!     started.elapsed(),
//!     "ln.backend" => "cln",
//! );
//! ```
//!
//! | `otlp.metrics` | `level` | directive | INFO metric | DEBUG metric |
//! |---|---|---|---|---|
//! | present | unset | `swgr::metrics=info` | recorded | dropped |
//! | present | `debug` | `swgr::metrics=debug` | recorded | recorded |
//! | present | `off` | `swgr::metrics=off` | dropped | dropped |
//! | absent, or `OTEL_METRICS_EXPORTER=none` | — | `swgr::metrics=off` | dropped | dropped |
//!
//! `otlp.metrics.level` is the sole control over metric verbosity at any
//! `RUST_LOG`. A `RUST_LOG` directive naming the metrics target is overridden.
//!
//! # Value types
//!
//! | Macro | Accepts | Recorded as |
//! |---|---|---|
//! | [`monotonic_counter!`] | `u8`–`u64`, `usize`, `f32`, `f64`, `Duration` | `u64` / `f64` |
//! | [`counter!`] | `i8`–`i64`, `isize`, `u8`–`u32`, `f32`, `f64` | `i64` / `f64` |
//! | [`histogram!`] | `u8`–`u64`, `usize`, `f32`, `f64`, `Duration` | `u64` / `f64` |
//! | [`gauge!`] | all integers, `f32`, `f64`, `Duration` | `u64` / `i64` / `f64` |
//!
//! A `Duration` records as milliseconds in `f64` — on [`histogram!`],
//! [`gauge!`] and [`monotonic_counter!`], but not [`counter!`].
//!
//! # Metric names
//!
//! Record a given metric name as one numeric type everywhere. Instrument maps
//! are keyed by name, so the same name recorded as `u64` in one place and
//! `f64` in another creates two instruments and splits the series.

mod macros;

#[doc(hidden)]
pub mod value;

#[doc(hidden)]
pub mod __private {
    pub use tracing;

    pub const TARGET: &str = "swgr::metrics";
}

/// The tracing target metric events are emitted on, for building subscriber
/// filters.
///
/// Constant: it does not vary by service, by whether metrics are enabled, or
/// at runtime.
pub fn metrics_target() -> &'static str {
    __private::TARGET
}
