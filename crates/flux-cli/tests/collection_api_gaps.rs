#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn collection_api_gaps_match_rustc() {
    support::oracle::compare_fixture("collection_api_gaps");
}
