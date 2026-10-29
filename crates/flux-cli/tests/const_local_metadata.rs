//! Mutable constant tables use live slice metadata without reading their elements.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn const_local_metadata_matches_rustc_off_and_on() {
    support::oracle::compare_fixture("const_local_metadata");
}
