//! Runtime function address exposure retains callable instances and their ABI.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn function_addresses_match_rustc() {
    support::oracle::compare_fixture("function_addresses");
}
