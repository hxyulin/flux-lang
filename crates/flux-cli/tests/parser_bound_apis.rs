//! Native contracts for conversion errors and borrowed range bounds.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn parser_bound_apis_match_rustc() {
    support::oracle::compare_fixture("parser_bound_apis");
}
