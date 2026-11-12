//! Tuple assignment preserves evaluation, moves, and destructor ordering.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn tuple_assignments_match_rustc() {
    support::oracle::compare_fixture("tuple_assignments");
}
