#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn nonzero_widening_matches_rustc() {
    support::oracle::compare_fixture("nonzero_widening");
}
