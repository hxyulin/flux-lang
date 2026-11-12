//! A consumed guard does not stay live through a shared conditional drop ladder.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn conditional_guard_drop_matches_rustc() {
    support::oracle::compare_fixture("conditional_guard_drop");
}
