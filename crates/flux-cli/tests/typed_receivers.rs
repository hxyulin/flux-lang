//! Explicit receiver signatures and anonymous trait imports execute unchanged.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn typed_receivers_match_rustc() {
    support::oracle::compare_fixture("typed_receivers");
}
