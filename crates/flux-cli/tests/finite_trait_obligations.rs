#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn finite_trait_obligations_match_rustc() {
    support::oracle::compare_fixture("finite_trait_obligations");
}
