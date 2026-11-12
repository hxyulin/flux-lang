#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn const_pointer_copy_matches_rustc() {
    support::oracle::compare_fixture("const_pointer_copy");
}
