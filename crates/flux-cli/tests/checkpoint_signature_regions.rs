#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn checkpoint_signatures_match_rustc() {
    support::oracle::compare_fixture("checkpoint_signature_regions");
}
