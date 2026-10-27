//! Static Self contracts support unchanged object-style marker traits.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn trait_static_superbounds_match_rustc() {
    support::oracle::compare_fixture("trait_static_superbounds");
}
