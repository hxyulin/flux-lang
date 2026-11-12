#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn error_downcasts_match_rustc() {
    support::oracle::compare_fixture("error_downcasts");
}
