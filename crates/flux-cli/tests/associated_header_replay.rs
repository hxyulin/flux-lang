#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn associated_header_replay_matches_rustc() {
    support::oracle::compare_fixture("associated_header_replay");
}
