//! Generated lint metadata retains Rust attribute validation and macro visibility.
use std::process::Command;

#[test]
fn automatically_derived_and_macro_exports_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-generated-metadata-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (source, expected) in [
        (
            "trait T{}struct S;#[automatically_derived]impl T for S{}",
            true,
        ),
        ("struct S;#[automatically_derived]impl S{}", true),
        ("#[automatically_derived]struct S;", true),
        ("#[automatically_derived]fn f(){}", true),
        (
            "trait T{}struct S;#[automatically_derived(x)]impl T for S{}",
            false,
        ),
        (
            "trait T{}struct S;#[automatically_derived=\"x\"]impl T for S{}",
            false,
        ),
        (
            "trait T{}struct S;#[unsafe(automatically_derived)]impl T for S{}",
            false,
        ),
        ("const TEXT:&str=std::stringify!(a+b);", true),
        (
            "#![no_std]extern crate std;const TEXT:&str=std::stringify!(a+b);",
            true,
        ),
        (
            "use std::stringify as text;const TEXT:&str=text!(a+b);",
            true,
        ),
        (
            "fn f(){use std::fmt::Write;let mut s=String::new();std::write!(&mut s,\"{}\",7).unwrap();}",
            true,
        ),
    ] {
        let path = root.join("case.rs");
        std::fs::write(&path, source).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--crate-type=lib", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("case.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            expected,
            "{source}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        assert_eq!(
            hir.diagnostics.is_empty(),
            expected,
            "{source}: {:?}",
            hir.diagnostics
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
