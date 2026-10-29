//! Contextual marker erasure inside clap-style error-source callbacks.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn inferred_object_casts_match_rustc_off_and_on() {
    support::oracle::compare_fixture("inferred_object_casts");
}
