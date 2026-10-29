//! Anonymous projection inputs are correlated by their original occurrences.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn method_projection_occurrences_match_rustc() {
    support::oracle::compare_fixture("method_projection_occurrences");
}
