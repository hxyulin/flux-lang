#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn backend_macro_admission_matches_rustc() {
    support::oracle::compare_fixture("backend_macro_admission");
}
