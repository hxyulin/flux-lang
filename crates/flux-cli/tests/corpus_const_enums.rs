//! Integer-represented const enums retain their declared discriminants.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn const_enums_match_rustc() {
    support::oracle::compare_fixture("corpus_const_enums");
}
