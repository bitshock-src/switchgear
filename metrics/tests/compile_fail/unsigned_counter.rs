// `counter.` drops a u64 above i64::MAX rather than recording it.

fn main() {
    switchgear_metrics::counter!("m", 1u64);
}
