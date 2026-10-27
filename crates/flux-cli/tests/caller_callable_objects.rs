//! Generic callback bounds remain universal when erased to dyn Fn objects.
mod support;
#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn caller_callable_objects_match_rustc() {
    support::oracle::compare_fixture("caller_callable_objects");
}
