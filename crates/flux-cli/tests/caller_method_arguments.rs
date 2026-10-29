//! Native caller-bound method selection with independent generic arguments.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn caller_method_arguments_match_rustc() {
    support::oracle::compare_fixture("caller_method_arguments");
}
