//! Auto-trait contracts and assertion wrappers retain ordinary callable behavior.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn unwind_safety_matches_rustc_off_and_on() {
    support::oracle::compare_fixture("unwind_safety");
}
