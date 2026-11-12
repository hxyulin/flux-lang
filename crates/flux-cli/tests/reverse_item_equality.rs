#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn reverse_item_equality_matches_rustc() {
    support::oracle::compare_fixture("reverse_item_equality");
}
