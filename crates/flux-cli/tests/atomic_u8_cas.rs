//! Byte compare-exchange observes Rust ordering and native contention semantics.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn byte_cas_matches_rustc_off_and_on() {
    support::oracle::compare_fixture("atomic_u8_cas");
}
