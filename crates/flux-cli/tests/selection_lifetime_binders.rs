#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn selection_lifetime_binders_match_rustc() {
    support::oracle::compare_fixture("selection_lifetime_binders");
}
