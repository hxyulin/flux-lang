#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn inherent_bound_fallback_matches_rustc() {
    support::oracle::compare_fixture("inherent_bound_fallback");
}
