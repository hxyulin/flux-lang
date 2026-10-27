//! Safe default methods preserve data loans independently of short receivers.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn safe_trait_defaults_match_rustc() {
    support::oracle::compare_fixture("safe_trait_defaults");
}
