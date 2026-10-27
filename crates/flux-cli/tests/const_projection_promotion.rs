//! Pure const substitutions and projections qualify inside nested promoted tables.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn const_projection_promotion_matches_rustc_off_and_on() {
    support::oracle::compare_fixture("const_projection_promotion");
}
