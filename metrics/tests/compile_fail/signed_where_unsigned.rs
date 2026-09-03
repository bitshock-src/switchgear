// `histogram.` has no i64 arm in tracing-opentelemetry, and
// `monotonic_counter.` casts a signed value to u64.

fn main() {
    switchgear_metrics::histogram!("m", -3i64);
    switchgear_metrics::monotonic_counter!("m", -1i32);
}
