#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn caller_object_bounds_match_rustc() {
    support::oracle::compare_fixture("caller_object_bounds");
}
