//! Static object occurrence constraints preserve pointer and ownership behavior.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn static_object_values_match_rustc() {
    support::oracle::compare_fixture("dyn_static_objects");
}
