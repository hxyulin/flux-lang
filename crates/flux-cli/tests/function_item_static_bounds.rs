//! Static type bounds do not bind independent function-item input lifetimes.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn function_item_static_bounds_match_rustc_off_and_on() {
    support::oracle::compare_fixture("function_item_static_bounds");
}
