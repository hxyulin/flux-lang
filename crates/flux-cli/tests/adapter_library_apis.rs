//! Adapter formatting and ownership-preserving result/slice helpers.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn adapter_apis_match_rustc_off_and_on() {
    support::oracle::compare_fixture("adapter_library_apis");
}
