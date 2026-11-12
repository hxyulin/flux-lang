#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn hash_constructor_bounds_match_rustc() {
    support::oracle::compare_fixture("hash_constructor_bounds");
}
