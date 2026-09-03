// A Duration is not an up-down counter value.

use std::time::Duration;

fn main() {
    switchgear_metrics::counter!("m", Duration::from_secs(1));
}
