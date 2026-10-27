//! Enum constant patterns compare tags and active fields rather than PartialEq.
mod support;
#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn enum_constant_patterns_match_rustc() {
    support::oracle::compare_fixture("enum_constant_patterns");
}
