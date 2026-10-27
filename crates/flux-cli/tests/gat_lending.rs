//! Associated families preserve lender, data and receiver lifetimes at runtime.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn gat_lending_matches_rustc() {
    support::oracle::compare_fixture("gat_lending");
}
