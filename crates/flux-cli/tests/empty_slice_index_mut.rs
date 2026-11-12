#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn empty_slice_index_mut_matches_rustc() {
    support::oracle::compare_fixture("empty_slice_index_mut");
}
