//! Typed parser supertrait bounds retain their generic conversion contracts.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn typed_value_parser_bounds_match_rustc() {
    support::oracle::compare_fixture("typed_value_parser_bounds");
}
