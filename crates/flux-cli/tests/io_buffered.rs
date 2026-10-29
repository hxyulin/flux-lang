//! Buffered readers preserve raw bytes, limits, EOF and fallible-reader state.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn buffered_readers_match_rustc_off_and_on() {
    support::oracle::compare_fixture("io_buffered");
}
