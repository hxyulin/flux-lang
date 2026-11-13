#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn enum_discriminant_macros_match_rustc() {
    support::oracle::compare_fixture("enum_discriminant_macros");
}
