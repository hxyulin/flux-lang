#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn corpus_ascii_apis_match_rustc() {
    support::oracle::compare_fixture("corpus_ascii_apis");
}
