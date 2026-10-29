//! Projected ordinary Parser headers preserve input allocation and mutability.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn ordinary_headers_match_rustc_off_and_on() {
    support::oracle::compare_fixture("ordinary_header_projections");
}
