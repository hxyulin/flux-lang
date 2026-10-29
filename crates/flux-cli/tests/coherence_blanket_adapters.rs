//! Native selection after proving blanket adapter intersections disjoint.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn blanket_adapters_match_rustc() {
    support::oracle::compare_fixture("coherence_blanket_adapters");
}
