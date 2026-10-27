//! Signed and halfword atomic representations required by Serde.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn atomic_family_matches_rustc_off_and_on() {
    support::oracle::compare_fixture("atomic_family");
}
