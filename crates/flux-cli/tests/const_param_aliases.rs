#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn const_param_aliases_match_rustc() {
    support::oracle::compare_fixture("const_param_aliases");
}
