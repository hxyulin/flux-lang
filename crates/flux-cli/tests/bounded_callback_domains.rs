#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn bounded_callback_domains_match_rustc() {
    support::oracle::compare_fixture("bounded_callback_domains");
}
