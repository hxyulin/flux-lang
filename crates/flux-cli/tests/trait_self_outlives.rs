#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn trait_self_outlives_matches_rustc() {
    support::oracle::compare_fixture("trait_self_outlives");
}
