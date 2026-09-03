//! Compile-fail coverage for the value traits and the const-key requirement.
//!
//! These assert on rustc's rendered diagnostics, so the `.stderr` files are
//! tied to the toolchain pinned in `rust-toolchain.toml` — and, for the E0435
//! case, to `tracing`'s own macro expansion. On a toolchain or `tracing` bump,
//! re-record them with `TRYBUILD=overwrite cargo test -p switchgear-metrics
//! --test ui` and read the diff: the point of the fixtures is that a call site
//! passing the wrong numeric type is told *why*, not merely that it failed.

#[test]
fn compile_fail() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/compile_fail/*.rs");
}
