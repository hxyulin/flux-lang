#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn adt_self_owner_matches_rustc() {
    support::oracle::compare_fixture("adt_self_owner");
}
