//! Large Rust aggregates cross every call form without expanding all enum variants.
mod support;

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn large_rust_arguments_match_rustc_off_and_on() {
    support::oracle::compare_fixture("large_rust_arguments");
    support::oracle::compare_fixture("empty_enum_leaves");
}
