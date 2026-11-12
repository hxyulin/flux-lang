//! Library methods used by the unchanged itertools source graph.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn itertools_library_apis_match_rustc() {
    support::oracle::compare_fixture("itertools_library");
}
