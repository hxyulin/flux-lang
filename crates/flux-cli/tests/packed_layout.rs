//! Actual packed storage, unaligned value accesses, and aligned destruction.
mod support;
#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn packed_layout_matches_rustc() {
    support::oracle::compare_fixture("packed_layout");
}
