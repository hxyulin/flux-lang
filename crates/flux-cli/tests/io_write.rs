//! Byte writer contracts required by object-file streaming output.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn io_write_matches_rustc() {
    support::oracle::compare_fixture("io_write");
}
