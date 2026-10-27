//! Non-callable universal bounds remain universal through blanket supertraits.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn higher_ranked_trait_bounds_match_rustc() {
    support::oracle::compare_fixture("higher_ranked_trait_bounds");
}
