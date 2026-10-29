//! A serde-style two-origin reader and postcard-style named body annotation.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn scalar_trait_and_annotation_native() {
    support::oracle::compare_fixture("trait_scalar_consts_and_annotations");
}
