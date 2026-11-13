#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn coroutine_output_outlives_matches_rustc() {
    support::oracle::compare_fixture("coroutine_output_outlives");
}
