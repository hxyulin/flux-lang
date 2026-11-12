#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn corpus_path_apis_match_rustc() {
    support::oracle::compare_fixture("corpus_path_apis");
}
