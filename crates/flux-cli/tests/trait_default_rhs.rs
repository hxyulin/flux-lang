#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn trait_default_rhs_matches_rustc() {
    support::oracle::compare_fixture("trait_default_rhs");
}
