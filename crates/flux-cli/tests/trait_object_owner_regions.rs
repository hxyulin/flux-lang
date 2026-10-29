//! thiserror-style object bounds and Visit-style long-lived node arguments.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn trait_object_owner_native() {
    support::oracle::compare_fixture("trait_object_owner_regions");
}
