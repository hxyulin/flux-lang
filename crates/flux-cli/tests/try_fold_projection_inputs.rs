//! Borrowed JSON-pointer-shaped traversal retains Try::Output input rows.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn try_fold_projection_inputs_match_rustc() {
    support::oracle::compare_fixture("try_fold_projection_inputs");
}
