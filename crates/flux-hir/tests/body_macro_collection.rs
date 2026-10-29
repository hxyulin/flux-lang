//! Body expression expansions keep their generated items in lexical scopes.
#[test]
fn expression_macro_items_are_collected() {
    for (name, source, accepted) in [
        (
            "local_block",
            "macro_rules!make{()=>{{use core::mem::size_of;size_of::<u8>()}}}fn f(){let _=make!();}",
            true,
        ),
        (
            "closure_block",
            "macro_rules!make{($e:expr)=>{identity(move|x:u8|{use core::mem::size_of;$e+size_of::<u8>() as u8+x})}}fn identity<F>(f:F)->F{f}fn f(){let _=make!(1);}",
            true,
        ),
        (
            "late_export",
            "fn f(){let _=crate::make!();}#[macro_export]macro_rules!make{()=>{{use core::mem::size_of;size_of::<u8>()}}}",
            true,
        ),
        (
            "nested",
            "macro_rules!inner{()=>{{use core::mem::size_of;size_of::<u8>()}}}macro_rules!outer{()=>{inner!()}}fn f(){let _=outer!();}",
            true,
        ),
        (
            "late_module_import",
            "mod early{use crate::later::make;pub fn f(){let _=make!();}}mod later{pub use crate::generated as make;}#[macro_export]macro_rules!generated{()=>{{use core::mem::size_of;size_of::<u8>()}}}",
            true,
        ),
        (
            "local_textual_forward",
            "fn f(){let _=make!();macro_rules!make{()=>{{use core::mem::size_of;size_of::<u8>()}}}}",
            false,
        ),
        (
            "root_textual_forward",
            "fn f(){let _=make!();}macro_rules!make{()=>{{use core::mem::size_of;size_of::<u8>()}}}",
            false,
        ),
        ("missing", "fn f(){let _=crate::missing!();}", false),
        (
            "private_macro",
            "fn f(){let _=later::make!();}mod later{macro_rules!make{()=>{{struct S;S}}}}",
            false,
        ),
        (
            "duplicate_item",
            "fn f(){struct S;crate::make!();}#[macro_export]macro_rules!make{()=>{struct S;}}",
            false,
        ),
        (
            "separate_expansions",
            "fn f(){let _=crate::make!();let _=crate::make!();}#[macro_export]macro_rules!make{()=>{{struct S;S}}}",
            true,
        ),
        (
            "nested_late_exports",
            "fn f(){let _=crate::outer!();}#[macro_export]macro_rules!outer{()=>{{crate::inner!()}}}#[macro_export]macro_rules!inner{()=>{{use core::mem::size_of;size_of::<u8>()}}}",
            true,
        ),
        (
            "generated_type_and_derive",
            "fn f(){let _=crate::make!();}#[macro_export]macro_rules!make{()=>{{#[derive(Clone,Copy)]struct S(u8);S(7)}}}",
            true,
        ),
    ] {
        let root =
            std::env::temp_dir().join(format!("flux-body-macro-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("case.rs");
        std::fs::write(&path, source).unwrap();
        let rustc = std::process::Command::new("rustc")
            .args(["--edition=2024", "--crate-type=lib", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("case.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "{name}: {source}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text("case.rs", source);
        assert_eq!(
            hir.diagnostics.is_empty(),
            accepted,
            "{name}: {:?}",
            hir.diagnostics
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn namespace_preview_does_not_register_impls_or_discard_errors() {
    let source = "trait Marker{}struct S;impl Marker for S{}fn f(){let _=crate::make!();}#[macro_export]macro_rules!make{()=>{{use core::mem::size_of;size_of::<u8>()}}}";
    let (_, _, hir) = flux_hir::lower_text("case.rs", source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let trait_id = hir
        .items
        .traits
        .iter()
        .find(|(_, t)| t.name.as_str() == "Marker")
        .unwrap()
        .0;
    assert_eq!(
        hir.items.trait_impls[&trait_id].len(),
        1,
        "namespace preview duplicated impl registration"
    );
    let bad = format!("#[repr(C,transparent)]struct Invalid(u8);{source}");
    let (_, _, hir) = flux_hir::lower_text("bad.rs", &bad);
    assert!(
        hir.diagnostics
            .iter()
            .any(|e| e.message.contains("transparent")),
        "lost preexisting attribute diagnostic: {:?}",
        hir.diagnostics
    );
}

#[test]
fn delayed_import_keeps_definition_edition() {
    use flux_hir::{SourceCrate, SourceDb};
    use flux_intern::Symbol;
    use flux_syntax::Edition;
    let root =
        std::env::temp_dir().join(format!("flux-delayed-mixed-edition-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (edition, edition_name) in [(Edition::E2018, "2018"), (Edition::E2024, "2024")] {
        let definition = "#[macro_export]macro_rules!make{($e:expr)=>{{use core::mem::size_of;($e,size_of::<u8>())}}}";
        let dep_path = root.join("dep.rs");
        std::fs::write(&dep_path, definition).unwrap();
        let library = root.join("libdefs.rlib");
        let rustc = std::process::Command::new("rustc")
            .arg(format!("--edition={edition_name}"))
            .args(["--crate-name=defs", "--crate-type=rlib"])
            .arg(&dep_path)
            .arg("-o")
            .arg(&library)
            .output()
            .unwrap();
        assert!(
            rustc.status.success(),
            "{}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        for (expression, accepted) in [("7", true), ("const { 7 }", edition_name == "2024")] {
            let consumer = format!(
                "mod early{{use crate::later::make;pub fn f(){{let _=make!({expression});}}}}mod later{{pub use defs::make;}}"
            );
            let path = root.join("main.rs");
            std::fs::write(&path, &consumer).unwrap();
            let rustc = std::process::Command::new("rustc")
                .args(["--edition=2024", "--crate-type=lib", "--emit=metadata"])
                .arg(&path)
                .arg("--extern")
                .arg(format!("defs={}", library.display()))
                .arg("-o")
                .arg(root.join("case.rmeta"))
                .output()
                .unwrap();
            assert_eq!(
                rustc.status.success(),
                accepted,
                "edition {edition_name} {expression}: {}",
                String::from_utf8_lossy(&rustc.stderr)
            );
            let mut db = SourceDb::new();
            let dep = db.add_with_edition(&dep_path, definition, edition);
            let main = db.add(&path, &consumer);
            let hir = flux_hir::lower_sources(
                &mut db,
                &[
                    SourceCrate {
                        root: dep,
                        name: Symbol::intern("defs"),
                        dependencies: vec![],
                    },
                    SourceCrate {
                        root: main,
                        name: Symbol::intern("consumer"),
                        dependencies: vec![(Symbol::intern("defs"), 0)],
                    },
                ],
            );
            assert_eq!(
                hir.diagnostics.is_empty(),
                accepted,
                "edition {edition_name} {expression}: {:?}",
                hir.diagnostics
            );
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
