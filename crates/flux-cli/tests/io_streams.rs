//! Read and seek contracts required by object-file read caches.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn io_streams_match_rustc() {
    support::oracle::compare_fixture("io_streams");
}
