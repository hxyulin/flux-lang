//! Transparent marker fields preserve physical layout, niches and ownership.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn transparent_structs_match_rustc() {
    support::oracle::compare_fixture("transparent_structs");
}
