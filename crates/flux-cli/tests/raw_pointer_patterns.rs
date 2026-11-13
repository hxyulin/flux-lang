#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn raw_pointer_patterns_match_rustc() {
    support::oracle::compare_fixture("raw_pointer_patterns");
}
