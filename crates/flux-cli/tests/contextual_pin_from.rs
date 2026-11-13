#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn contextual_pin_from_matches_rustc() {
    support::oracle::compare_fixture("contextual_pin_from");
}
