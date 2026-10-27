//! Test-only definitions are absent from ordinary native programs.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn test_attributes_match_rustc() {
    support::oracle::compare_fixture("test_attributes");
}
