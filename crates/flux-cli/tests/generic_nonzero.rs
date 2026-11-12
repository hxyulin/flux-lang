#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn generic_nonzero_matches_rustc() {
    support::oracle::compare_fixture("generic_nonzero");
}
