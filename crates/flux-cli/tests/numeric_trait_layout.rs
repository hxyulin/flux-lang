#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn numeric_trait_layout_matches_rustc() {
    support::oracle::compare_fixture("numeric_trait_layout");
}
