//! Forwarded output lifetime contracts retain the actual owner borrow.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn forwarded_family_outlives_matches_rustc() {
    support::oracle::compare_fixture("declaration_family_outlives");
}
