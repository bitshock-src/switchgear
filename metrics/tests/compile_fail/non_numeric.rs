// A metric value has to be a number.

fn main() {
    switchgear_metrics::histogram!("m", "nope");
}
