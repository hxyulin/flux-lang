//! Renamed extern crates retain their source identity and lexical visibility.
use std::process::Command;

#[test]
fn extern_aliases_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-extern-aliases-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let accepted = [
        "extern crate alloc as heap; fn f() -> heap::vec::Vec<u8> { heap::vec::Vec::new() }",
        "#![no_std] extern crate alloc as heap; mod child { fn f() -> heap::vec::Vec<u8> { ::heap::vec::Vec::new() } }",
        "mod inner { extern crate alloc as heap; fn f() -> heap::vec::Vec<u8> { heap::vec::Vec::new() } }",
        "mod inner { pub extern crate alloc as heap; } fn f() -> inner::heap::vec::Vec<u8> { inner::heap::vec::Vec::new() }",
        "mod inner { pub extern crate alloc; } fn f() { inner::alloc::vec::Vec::<u8>::new(); }",
        "mod inner { extern crate alloc; fn f() { alloc::vec::Vec::<u8>::new(); } }",
        "extern crate core as foundation; use foundation::num::NonZeroU32; fn f() -> Option<NonZeroU32> { NonZeroU32::new(1) }",
        "mod control { mod tag { pub struct Tag(pub u8); } use self::{tag::{Tag}}; fn f() -> Tag { Tag(7) } }",
    ];
    let rejected = [
        "extern crate missing as heap;",
        "mod inner { extern crate alloc as heap; } fn f() { heap::vec::Vec::<u8>::new(); }",
        "mod inner { extern crate alloc as heap; } fn f() { inner::heap::vec::Vec::<u8>::new(); }",
        "#![no_std] extern crate alloc as heap; fn f() { alloc::vec::Vec::<u8>::new(); }",
        "extern crate alloc as heap; mod heap {}",
        "mod inner { extern crate alloc; } fn f() { alloc::vec::Vec::<u8>::new(); }",
        "mod inner { extern crate alloc; } fn f() { inner::alloc::vec::Vec::<u8>::new(); }",
    ];
    for (sources, expected) in [(&accepted[..], true), (&rejected[..], false)] {
        for source in sources {
            let path = root.join("probe.rs");
            std::fs::write(&path, source).unwrap();
            let oracle = Command::new("rustc")
                .args(["--edition=2024", "--crate-type=lib", "--emit=metadata"])
                .arg(&path)
                .arg("-o")
                .arg(root.join("probe.rmeta"))
                .output()
                .unwrap();
            assert_eq!(
                oracle.status.success(),
                expected,
                "{source}: {}",
                String::from_utf8_lossy(&oracle.stderr)
            );
            let (db, file, hir) = flux_hir::lower_text(&path, source);
            assert!(db.file(file).parse.errors().is_empty());
            assert_eq!(
                hir.diagnostics.is_empty(),
                expected,
                "{source}: {:?}",
                hir.diagnostics
            );
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn transparent_wrappers_and_repr_syntax_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-transparent-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (source, expected) in [
        ("#[repr(transparent)] struct Tag(u8);", true),
        ("#[repr(transparent)] struct Wrapper<T>(T);", true),
        ("#[repr(transparent)] struct Unit(());", true),
        (
            "#[repr(transparent)] struct Marker<T>(u8, core::marker::PhantomData<T>);",
            true,
        ),
        ("#[repr(transparent, C)] struct Invalid(u8);", false),
        ("#[repr(transparent, align(8))] struct Invalid(u8);", false),
        ("#[repr(transparent)] union Invalid { value: u8 }", false),
    ] {
        let path = root.join("probe.rs");
        std::fs::write(&path, source).unwrap();
        let oracle = Command::new("rustc")
            .args(["--edition=2024", "--crate-type=lib", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("probe.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            expected,
            "{source}: {}",
            String::from_utf8_lossy(&oracle.stderr)
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

#[test]
fn deprecation_attributes_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-deprecated-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (attribute, accepted) in [
        ("#[deprecated]", true),
        ("#[deprecated = \"use another function\"]", true),
        (
            "#[deprecated(note = \"use another function\", since = \"1.0.0\",)]",
            true,
        ),
        ("#[deprecated(note = 7)]", false),
        ("#[deprecated(note = \"a\", note = \"b\")]", false),
        ("#[deprecated(reason = \"a\")]", false),
        ("#[unsafe(deprecated)]", false),
    ] {
        let source = format!("{attribute} fn old() {{}}");
        let path = root.join("probe.rs");
        std::fs::write(&path, &source).unwrap();
        let oracle = Command::new("rustc")
            .args(["--edition=2024", "--crate-type=lib", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("probe.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "{source}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        assert_eq!(
            hir.diagnostics.is_empty(),
            accepted,
            "{source}: {:?}",
            hir.diagnostics
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
