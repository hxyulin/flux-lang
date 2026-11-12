#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn neon_corpus_matches_rustc() {
    support::oracle::compare_fixture("neon_corpus");
}
