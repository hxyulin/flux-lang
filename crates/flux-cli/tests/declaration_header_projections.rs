//! A selected projected header and nested iterator RHS use actual input rows.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn projected_headers_match_rustc() {
    support::oracle::compare_fixture("declaration_header_projections");
}
