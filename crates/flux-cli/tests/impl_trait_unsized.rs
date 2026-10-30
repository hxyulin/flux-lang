//! Relaxed anonymous parameters and imported primitive types retain Rust semantics.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn unsized_anonymous_parameters_match_rustc() {
    support::oracle::compare_fixture("impl_trait_unsized");
}
