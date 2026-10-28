#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn array_length_constants_match_rustc() {
    support::oracle::compare_fixture("array_length_constants");
}
