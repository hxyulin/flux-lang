//! Standard handles share buffering and native terminal/descriptor semantics.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn standard_streams_match_rustc_off_and_on() {
    support::oracle::compare_fixture("io_stdio");
}
