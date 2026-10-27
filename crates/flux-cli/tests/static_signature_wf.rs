//! Signature ADT well-formedness implies its explicit static contracts.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn static_signature_wf_matches_rustc() {
    support::oracle::compare_fixture("static_signature_wf");
}
