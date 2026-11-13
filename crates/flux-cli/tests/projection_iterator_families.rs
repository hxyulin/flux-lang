#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn projection_iterator_families_match_rustc() {
    support::oracle::compare_fixture("projection_iterator_families");
}
