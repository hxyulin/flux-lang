#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn caller_projection_regions_match_rustc() {
    support::oracle::compare_fixture("caller_projection_regions");
}
