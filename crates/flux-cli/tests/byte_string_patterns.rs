//! Byte string patterns execute through array and slice projections.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn byte_string_patterns_match_rustc() {
    support::oracle::compare_fixture("byte_string_patterns");
}
