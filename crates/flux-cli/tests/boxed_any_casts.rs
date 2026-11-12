//! Boxed object casts preserve ownership and dynamic type identity.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn boxed_any_casts_match_rustc() {
    support::oracle::compare_fixture("boxed_any_casts");
}
