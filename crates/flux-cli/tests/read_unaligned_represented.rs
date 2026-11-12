#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn read_unaligned_represented_matches_rustc() {
    support::oracle::compare_fixture("read_unaligned_represented");
}
