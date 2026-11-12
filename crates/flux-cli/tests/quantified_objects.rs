//! Higher-ranked ordinary trait objects retain per-call equality lifetimes.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn quantified_objects_match_rustc() {
    support::oracle::compare_fixture("quantified_objects");
}
