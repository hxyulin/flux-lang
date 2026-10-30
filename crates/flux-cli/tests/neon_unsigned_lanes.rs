//! Unsigned lane masks, wrapping arithmetic, reductions and unaligned loads.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn unsigned_neon_lanes_match_rustc() {
    support::oracle::compare_fixture("neon_unsigned_lanes");
}
