#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn local_type_annotation_scopes_match_rustc() {
    support::oracle::compare_fixture("local_type_annotation_scopes");
}
