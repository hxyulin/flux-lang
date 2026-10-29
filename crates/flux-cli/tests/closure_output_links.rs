//! Callable Output lifetime links survive generic and opaque adapters.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn closure_output_links_match_rustc_off_and_on() {
    support::oracle::compare_fixture("closure_output_links");
}
