//! A bound-dependent pointer output may change layout after selecting an impl.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn family_pointer_outputs_match_rustc() {
    support::oracle::compare_fixture("family_pointer_output");
}
