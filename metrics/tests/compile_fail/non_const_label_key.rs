// A label key becomes part of the callsite's 'static metadata, so it has to
// be a constant expression: E0435.

// `key` reads as used here, but the expansion only reaches it from a `static`
// initialiser, so rustc reports it unused as well. That warning is noise on
// top of the error under test.
#[allow(unused_variables)]
fn main() {
    let key = "ln.backend";
    switchgear_metrics::histogram!("m", 1u64, key => "cln");
}
