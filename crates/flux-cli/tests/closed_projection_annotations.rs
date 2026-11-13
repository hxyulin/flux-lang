#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn closed_projection_annotations_match_rustc() {
    support::oracle::compare_fixture("closed_projection_annotations");
}
