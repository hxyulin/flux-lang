#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn cow_slice_equality_matches_rustc() {
    support::oracle::compare_fixture("cow_slice_equality");
}
