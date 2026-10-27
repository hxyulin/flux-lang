//! Native data flow through nominal types with region-bearing trait bounds.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn bounded_adt_data_flow_matches_rustc() {
    support::oracle::compare_fixture("adt_trait_regions");
}
