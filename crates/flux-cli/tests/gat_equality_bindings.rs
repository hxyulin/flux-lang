//! Associated family equality arguments drive real array fields and ownership.
mod support;
#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn gat_equality_bindings_match_rustc() {
    support::oracle::compare_fixture("gat_equality_bindings");
}
