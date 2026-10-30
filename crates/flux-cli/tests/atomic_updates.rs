//! Atomic updates retain scalar widths, old values and pointer representations.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn atomic_updates_match_rustc() {
    support::oracle::compare_fixture("atomic_updates");
}
