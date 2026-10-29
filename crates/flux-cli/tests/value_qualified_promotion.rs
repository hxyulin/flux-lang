//! Promotion qualifies only the constructed enum variant and its actual fields.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn value_qualified_promotion_matches_rustc_off_and_on() {
    support::oracle::compare_fixture("value_qualified_promotion");
}
