//! Native Linux coverage stays explicit until the full runtime is ported.
#![cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod support;

#[test]
fn x86_64_programs_match_rustc() {
    for name in [
        "backend_full_selection",
        "btree_map",
        "pointer_slice_helpers",
        "atomic_family",
        "monotonic_time",
        "backend_system_time",
        "unix_file_io",
        "scoped_threads",
        "thread_local",
    ] {
        support::oracle::compare_fixture(name);
    }
}
