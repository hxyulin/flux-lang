//! Native static/borrowed fields and object principal declaration proofs.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn structural_declarations_match_rustc() {
    support::oracle::compare_fixture("declaration_structural_rows");
}
