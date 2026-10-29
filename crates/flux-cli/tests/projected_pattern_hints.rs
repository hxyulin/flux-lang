//! Deferred parser projections constrain nested patterns and closure annotations.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn projected_pattern_hints_match_rustc_off_and_on() {
    support::oracle::compare_fixture("projected_pattern_hints");
}
