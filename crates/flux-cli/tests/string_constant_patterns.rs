//! Named string patterns compare contents of independently owned strings.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn string_constants_match_rustc_off_and_on() {
    support::oracle::compare_fixture("string_constant_patterns");
}
