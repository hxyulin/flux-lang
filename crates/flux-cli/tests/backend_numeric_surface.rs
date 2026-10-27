#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn backend_numeric_surface_matches_rustc() {
    support::oracle::compare_fixture("backend_numeric_surface");
}
