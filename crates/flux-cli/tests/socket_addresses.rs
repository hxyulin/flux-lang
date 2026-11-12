//! Standard address conversion, host lookup and fallible string growth.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn socket_addresses_match_rustc() {
    support::oracle::compare_fixture("socket_addresses");
}
