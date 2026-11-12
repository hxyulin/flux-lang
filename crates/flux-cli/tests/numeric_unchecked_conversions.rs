#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn numeric_unchecked_conversions_match_rustc() {
    support::oracle::compare_fixture("numeric_unchecked_conversions");
}
