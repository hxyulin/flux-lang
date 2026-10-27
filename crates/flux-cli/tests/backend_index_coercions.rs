#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn backend_index_coercions_match_rustc() {
    support::oracle::compare_fixture("backend_index_coercions");
}
