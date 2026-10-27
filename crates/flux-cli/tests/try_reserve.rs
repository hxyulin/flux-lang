//! Fallible vector growth preserves values and matches the allocation error contract.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn try_reserve_matches_rustc() {
    support::oracle::compare_fixture("try_reserve");
}
