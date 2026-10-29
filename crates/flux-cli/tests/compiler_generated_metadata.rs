//! Generated lint metadata and standard macro reexports retain runtime behavior.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn generated_metadata_matches_rustc() {
    support::oracle::compare_fixture("compiler_generated_metadata");
}
