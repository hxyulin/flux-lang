//! Named byte-slice patterns compare contents and preserve exhaustiveness.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn byte_slice_patterns_match_rustc() {
    support::oracle::compare_fixture("byte_slice_patterns");
}
