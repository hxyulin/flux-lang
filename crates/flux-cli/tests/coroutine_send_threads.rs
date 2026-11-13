#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn coroutine_send_threads_match_rustc() {
    support::oracle::compare_fixture("coroutine_send_threads");
}
