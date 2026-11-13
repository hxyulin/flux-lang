#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn captureless_closure_promotion_matches_rustc() {
    support::oracle::compare_fixture("captureless_closure_promotion");
}
