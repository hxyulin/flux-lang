//! Ordinary native fixtures share one binary; each remains a separately named test.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn aggregate_literal_promotion() {
    support::oracle::compare_fixture("aggregate_literal_promotion");
}

#[test]
fn assembler_library_prerequisites() {
    support::oracle::compare_fixture("ordered_set");
}

#[test]
fn atomic_u64() {
    support::oracle::compare_fixture("atomic_u64");
}

#[test]
fn bumpalo_library_prerequisites() {
    support::oracle::compare_fixture("bumpalo_library_prerequisites");
}

#[test]
fn cow_as_ref() {
    support::oracle::compare_fixture("cow_as_ref");
}

#[test]
fn default_hash_collections() {
    support::oracle::compare_fixture("default_hash_collections");
}

#[test]
fn keel_library_prerequisites() {
    support::oracle::compare_fixture("keel_library_prerequisites");
}

#[test]
fn large_rust_arguments() {
    support::oracle::compare_fixture("large_rust_arguments");
}

#[test]
fn nested_inherited_projections() {
    support::oracle::compare_fixture("nested_inherited_projections");
}

#[test]
fn parameter_defaults() {
    support::oracle::compare_fixture("type_parameter_defaults");
}

#[test]
fn projected_pointer_regions() {
    support::oracle::compare_fixture("projected_pointer_regions");
}

#[test]
fn trait_projection_owners() {
    support::oracle::compare_fixture("trait_projection_owners");
}

#[test]
fn union_pointer_fields() {
    support::oracle::compare_fixture("union_pointer_fields");
}

#[test]
fn repeat_autoref_promotion() {
    support::oracle::compare_fixture("repeat_autoref_promotion");
}

#[test]
fn str_ascii_owned() {
    support::oracle::compare_fixture("str_ascii_owned");
}

#[test]
fn caller_bound_environments() {
    support::oracle::compare_fixture("caller_bound_environments");
}

#[test]
fn copy_const_drops() {
    support::oracle::compare_fixture("copy_const_drops");
}

#[test]
fn nested_associated_bounds() {
    support::oracle::compare_fixture("nested_associated_bounds");
}

#[test]
fn slice_callback_lifetimes() {
    support::oracle::compare_fixture("slice_callback_lifetimes");
}

#[test]
fn backend_std_prerequisites() {
    support::oracle::compare_fixture("backend_std_prerequisites");
}

#[test]
fn backend_owned_apis() {
    support::oracle::compare_fixture("backend_owned_apis");
}
