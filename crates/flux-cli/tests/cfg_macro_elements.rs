#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn cfg_macro_elements_match_rustc() {
    support::oracle::compare_fixture("cfg_macro_elements");
}
