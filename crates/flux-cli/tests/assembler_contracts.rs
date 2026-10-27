//! Standard library contracts exercised by the assembler dependencies.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn assembler_contracts_match_rustc() {
    support::oracle::compare_fixture("assembler_contracts");
}
