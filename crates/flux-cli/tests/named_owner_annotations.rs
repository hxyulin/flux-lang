#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn named_owner_annotations_match_rustc() {
    support::oracle::compare_fixture("named_owner_annotations");
}
#[test]
fn boxed_borrow_matches_rustc() {
    support::oracle::compare_fixture("boxed_borrow");
}
