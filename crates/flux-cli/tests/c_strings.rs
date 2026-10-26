//! C-string layout and byte operations share the Rust native oracle.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn c_strings_match_rustc() {
    support::oracle::compare_fixture("c_strings");
}

#[test]
fn c_string_borrows_and_pointer_metadata_match_rustc() {
    use std::process::Command;
    use std::time::Duration;

    let directory =
        std::env::temp_dir().join(format!("flux-cstr-contracts-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for (name, source, expected) in [
        (
            "borrowed",
            "use core::ffi::CStr; fn f<'a>(bytes: &'a [u8]) -> &'a CStr { CStr::from_bytes_with_nul(bytes).unwrap() }",
            true,
        ),
        (
            "escape",
            "use core::ffi::CStr; fn f() -> &'static CStr { let bytes = [b'x', 0]; CStr::from_bytes_with_nul(&bytes).unwrap() }",
            false,
        ),
        (
            "thin_pointer",
            "use core::ffi::CStr; fn f(p: *const u8) -> *const CStr { p as *const CStr }",
            false,
        ),
        (
            "integer_pointer",
            "use core::ffi::CStr; fn f(p: usize) -> *const CStr { p as *const CStr }",
            false,
        ),
        (
            "wide_integer",
            "use core::ffi::CStr; fn f(p: *const CStr) -> usize { p as usize }",
            false,
        ),
        (
            "data_address",
            "use core::ffi::CStr; fn f(p: *const CStr) -> usize { p as *const u8 as usize }",
            true,
        ),
        (
            "unsized_value",
            "use core::ffi::CStr; fn f() -> usize { core::mem::size_of::<CStr>() }",
            false,
        ),
    ] {
        let input = directory.join(format!("{name}.rs"));
        std::fs::write(&input, source).unwrap();
        let reference = support::process::run(
            Command::new("rustc")
                .args(["--edition=2024", "--crate-type=lib", "--emit=metadata"])
                .arg(&input)
                .arg("-o")
                .arg(directory.join(format!("{name}.rmeta"))),
            &directory,
            name,
            Duration::from_secs(30),
        )
        .completed();
        assert_eq!(reference.status.success(), expected, "rustc {name}");
        let (db, _, hir) = flux_hir::lower_text(format!("{name}.rs"), source);
        assert!(db.syntax_errors().is_empty(), "{name}");
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let accepted = typed.diagnostics.is_empty()
            && flux_mir::build_file(&hir, &typed).diagnostics.is_empty();
        assert_eq!(accepted, expected, "flux {name}");
    }
}
