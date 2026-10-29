#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn parameter_bound_priority_matches_rustc() {
    support::oracle::compare_fixture("parameter_bound_priority");
}
