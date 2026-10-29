//! Borrowed/mutable table iterator objects preserve their associated rows.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn object_arguments_match_rustc_off_and_on() {
    support::oracle::compare_fixture("object_argument_regions");
}
