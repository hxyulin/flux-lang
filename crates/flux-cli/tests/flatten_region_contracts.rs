//! Nested iterator projections retain the backing data's lifetime rows.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn flatten_region_contracts_match_rustc_off_and_on() {
    support::oracle::compare_fixture("flatten_region_contracts");
}
