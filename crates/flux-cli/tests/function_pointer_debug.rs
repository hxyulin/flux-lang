//! Function-pointer Debug observes addresses without calling the pointer.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn function_pointer_debug_matches_rustc_off_and_on() {
    support::oracle::compare_fixture("function_pointer_debug");
}
