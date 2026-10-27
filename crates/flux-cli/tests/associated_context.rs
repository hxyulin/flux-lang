//! Associated trait functions obtain Self from their call context.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn associated_context_matches_rustc() {
    support::oracle::compare_fixture("associated_context");
}
