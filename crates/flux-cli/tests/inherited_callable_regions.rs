//! Inherited callback argument rows survive selected supertrait methods.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn inherited_callable_arguments_match_rustc() {
    support::oracle::compare_fixture("inherited_callable_regions");
}
