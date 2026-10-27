//! Constant trait-object tables preserve concrete data and vtable reachability.
mod support;
#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn const_trait_objects_match_rustc() {
    support::oracle::compare_fixture("const_trait_objects");
}
