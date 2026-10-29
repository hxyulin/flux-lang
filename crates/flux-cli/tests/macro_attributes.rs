#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn macro_attributes_match_rustc() {
    support::oracle::compare_fixture("macro_attributes");
}
