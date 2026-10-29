//! Opaque bounds retain trait argument and associated-output lifetimes.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn opaque_bounds_match_rustc_off_and_on() {
    support::oracle::compare_fixture("opaque_bound_regions");
}
