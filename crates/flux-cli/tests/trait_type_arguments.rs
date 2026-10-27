//! Tracked trait type arguments preserve independent shared and mutable loans.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn named_trait_type_arguments_match_rustc() {
    support::oracle::compare_fixture("trait_type_arguments");
}
