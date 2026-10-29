//! Projections introduced by late concrete parser arguments normalize normally.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn parser_projection_arguments_match_rustc() {
    support::oracle::compare_fixture("parser_projection_arguments");
}
