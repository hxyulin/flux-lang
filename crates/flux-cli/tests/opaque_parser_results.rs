//! Opaque parser contracts keep borrowed results independent of the receiver.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn opaque_parser_results_match_rustc_off_and_on() {
    support::oracle::compare_fixture("opaque_parser_results");
}
