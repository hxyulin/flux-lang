//! A nested map closure returns only the region selected by its checked body.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn closure_result_origins_match_rustc() {
    support::oracle::compare_fixture("closure_result_origins");
}
