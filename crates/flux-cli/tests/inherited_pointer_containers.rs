//! Inherited iterator outputs can store independently checked function pointers.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn inherited_pointer_containers_match_rustc_off_and_on() {
    support::oracle::compare_fixture("inherited_pointer_containers");
}
