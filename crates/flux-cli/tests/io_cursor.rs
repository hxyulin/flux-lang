//! Seekable in-memory output used by the unchanged Keel linker.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn cursor_matches_rustc_off_and_on() {
    support::oracle::compare_fixture("io_cursor");
}
