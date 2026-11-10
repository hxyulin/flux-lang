//! Macro scope and expansion limits are invocation-crate inputs.
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use std::process::Command;

fn compare(
    name: &str,
    source: &str,
    child: Option<&str>,
    expected: bool,
    edition: flux_syntax::Edition,
) {
    let dir = std::env::temp_dir().join(format!(
        "flux-macro-attrs-{}-{name}-{edition:?}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("probe.rs");
    std::fs::write(&path, source).unwrap();
    if let Some(child) = child {
        std::fs::write(dir.join("child.rs"), child).unwrap();
    }
    let oracle = Command::new("rustc")
        .arg(format!(
            "--edition={}",
            match edition {
                flux_syntax::Edition::E2015 => "2015",
                _ => "2024",
            }
        ))
        .args(["--crate-type=lib", "--emit=metadata"])
        .arg(&path)
        .arg("-o")
        .arg(dir.join("probe.rmeta"))
        .output()
        .unwrap();
    assert_eq!(
        oracle.status.success(),
        expected,
        "{name}: {}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let mut db = SourceDb::new();
    let root = db.load_with_edition(&path, edition).unwrap();
    let hir = flux_hir::lower(&mut db, root, Symbol::intern("probe"));
    assert_eq!(
        db.syntax_errors().is_empty() && hir.diagnostics.is_empty(),
        expected,
        "{name}: {:?} {:?}",
        db.syntax_errors(),
        hir.diagnostics
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn macro_scope_attributes_match_rustc() {
    for edition in [flux_syntax::Edition::E2015, flux_syntax::Edition::E2024] {
        for (name, source, child, accepted) in [
            (
                "inert_module_attributes",
                "mod child { #![feature(rustc_attrs)] #[macro_use] use std::fmt::Debug; }",
                None,
                true,
            ),
            (
                "file",
                "mod child; make!();",
                Some("#![macro_use] macro_rules! make{()=>{pub struct Made;}}"),
                true,
            ),
            (
                "inline",
                "mod child{#![macro_use] macro_rules! make{()=>{pub struct Made;}}} make!();",
                None,
                true,
            ),
            (
                "outer",
                "#[macro_use] mod child; make!();",
                Some("macro_rules! make{()=>{pub struct Made;}}"),
                true,
            ),
            (
                "nested",
                "mod child{#![macro_use] mod inner{#![macro_use] macro_rules! make{()=>{pub struct Made;}}}} make!();",
                None,
                true,
            ),
            (
                "private",
                "mod child; make!();",
                Some("macro_rules! make{()=>{pub struct Made;}}"),
                false,
            ),
            (
                "before",
                "make!(); mod child;",
                Some("#![macro_use] macro_rules! make{()=>{pub struct Made;}}"),
                false,
            ),
            (
                "args",
                "mod child;",
                Some("#![macro_use(make)] macro_rules! make{()=>{}}"),
                false,
            ),
            ("inline_args", "mod child{#![macro_use(make)]}", None, false),
            (
                "primitive",
                "mod child{#![macro_use] macro_rules! make{()=>{type Word=u32;}}}make!(); fn f(_:Word){}",
                None,
                true,
            ),
            (
                "shadow",
                "macro_rules! make{()=>{missing!();}}mod child{#![macro_use]macro_rules! make{()=>{struct Made;}}}make!();",
                None,
                true,
            ),
        ] {
            compare(name, source, child, accepted, edition);
        }
    }
}

#[test]
fn recursion_limits_match_rustc() {
    for (name, source, accepted) in [
        ("zero_unused", "#![recursion_limit=\"0\"] struct S;", true),
        (
            "zero_macro",
            "#![recursion_limit=\"0\"] macro_rules! m{()=>{}}m!();",
            false,
        ),
        (
            "small_ok",
            "#![recursion_limit=\"8\"] macro_rules! a{()=>{b!();}}macro_rules! b{()=>{struct S;}}a!();",
            true,
        ),
        (
            "small_fail",
            "#![recursion_limit=\"1\"] macro_rules! a{()=>{b!();}}macro_rules! b{()=>{struct S;}}a!();",
            false,
        ),
        (
            "body_fail",
            "#![recursion_limit=\"1\"] macro_rules! a{()=>{b!()}}macro_rules! b{()=>{1}}fn f(){a!();}",
            false,
        ),
        (
            "eager_fail",
            "#![recursion_limit=\"1\"] const S:&str=concat!(concat!(\"x\"));",
            false,
        ),
        ("bad_kind", "#![recursion_limit=12]", false),
        ("bad_radix", "#![recursion_limit=\"0x10\"]", false),
        ("bad_separator", "#![recursion_limit=\"1_2\"]", false),
        ("large", "#![recursion_limit=\"4294967296\"]", true),
        ("plus", "#![recursion_limit=\"+4\"]", true),
        (
            "first_wins",
            "#![recursion_limit=\"0\"]#![recursion_limit=\"64\"]macro_rules! m{()=>{}}m!();",
            false,
        ),
        (
            "module_ignored",
            "mod child{#![recursion_limit=\"0\"]macro_rules! m{()=>{struct S;}}m!();}",
            true,
        ),
    ] {
        compare(name, source, None, accepted, flux_syntax::Edition::E2024);
    }
    let chain = (0..140)
        .map(|i| format!("macro_rules! m{i}{{()=>{{m{}!();}}}}", i + 1))
        .collect::<String>();
    compare(
        "raised",
        &format!("#![recursion_limit=\"256\"]{chain}macro_rules! m140{{()=>{{struct S;}}}}m0!();"),
        None,
        true,
        flux_syntax::Edition::E2024,
    );
    compare(
        "default",
        &format!("{chain}macro_rules! m140{{()=>{{struct S;}}}}m0!();"),
        None,
        false,
        flux_syntax::Edition::E2024,
    );
}

#[test]
fn dependency_macros_use_invocation_limit() {
    let directory = std::env::temp_dir().join(format!("flux-crate-limits-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for (dep_limit, caller_limit, accepted) in [(0, 8, true), (256, 0, false)] {
        let dep_source = format!(
            "#![recursion_limit=\"{dep_limit}\"]#[macro_export]macro_rules! m{{()=>{{struct S;}}}}"
        );
        let root_source = format!("#![recursion_limit=\"{caller_limit}\"]dep::m!();");
        let dep_path = directory.join("dep.rs");
        let root_path = directory.join("root.rs");
        std::fs::write(&dep_path, &dep_source).unwrap();
        std::fs::write(&root_path, &root_source).unwrap();
        let library = directory.join("libdep.rlib");
        let dependency = Command::new("rustc")
            .args(["--edition=2024", "--crate-type=rlib", "--crate-name=dep"])
            .arg(&dep_path)
            .arg("-o")
            .arg(&library)
            .output()
            .unwrap();
        assert!(
            dependency.status.success(),
            "{}",
            String::from_utf8_lossy(&dependency.stderr)
        );
        let reference = Command::new("rustc")
            .args([
                "--edition=2024",
                "--crate-type=lib",
                "--emit=metadata",
                "--extern",
            ])
            .arg(format!("dep={}", library.display()))
            .arg(&root_path)
            .arg("-o")
            .arg(directory.join("root.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "{}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let mut db = SourceDb::new();
        let dep = db.add(&dep_path, &dep_source);
        let root = db.add(&root_path, &root_source);
        let hir = flux_hir::lower_sources(
            &mut db,
            &[
                SourceCrate {
                    root: dep,
                    name: Symbol::intern("dep"),
                    dependencies: vec![],
                },
                SourceCrate {
                    root,
                    name: Symbol::intern("root"),
                    dependencies: vec![(Symbol::intern("dep"), 0)],
                },
            ],
        );
        assert_eq!(
            hir.diagnostics.is_empty(),
            accepted,
            "{:?}",
            hir.diagnostics
        );
        for (name, limit) in [("dep", dep_limit), ("root", caller_limit)] {
            assert!(
                hir.items
                    .crates
                    .iter()
                    .any(|c| c.name == Symbol::intern(name) && c.recursion_limit == limit)
            );
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn type_length_limits_match_stable_rustc() {
    for (name, source, accepted) in [
        ("unused", "#![type_length_limit=\"0\"] struct S;", true),
        ("plus", "#![type_length_limit=\"+4\"]", true),
        ("kind", "#![type_length_limit=12]", false),
        ("radix", "#![type_length_limit=\"0x10\"]", false),
        ("separator", "#![type_length_limit=\"1_2\"]", false),
        ("negative", "#![type_length_limit=\"-1\"]", false),
        (
            "module",
            "mod m { #![type_length_limit=\"0\"] struct S; }",
            true,
        ),
    ] {
        compare(
            &format!("type_length_{name}"),
            source,
            None,
            accepted,
            flux_syntax::Edition::E2024,
        );
    }
}

#[test]
fn safe_foreign_items_survive_item_fragment_matching() {
    compare(
        "safe_foreign_item_fragment",
        r#"macro_rules! accept {($item:item)=>{$item}} accept!(unsafe extern "C" {safe fn count()->u32;});"#,
        None,
        true,
        flux_syntax::Edition::E2024,
    );
}
