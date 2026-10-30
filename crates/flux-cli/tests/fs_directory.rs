//! Native directory, metadata, executable-path and sleep contracts.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn directory_runtime_matches_rustc() {
    support::oracle::compare_fixture("fs_directory");
}
