#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn method_lifetime_binders_match_rustc() {
    support::oracle::compare_fixture("method_lifetime_binders");
}
