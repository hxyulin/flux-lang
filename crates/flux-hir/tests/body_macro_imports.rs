//! Imports made by body macros share the ordinary import resolver and scopes.
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;

fn dependency_errors(dependency: &str, consumer: &str) -> Vec<String> {
    let mut db = SourceDb::new();
    let dep = db.add("/body-imports/dep/lib.rs", dependency);
    let main = db.add("/body-imports/main.rs", consumer);
    let hir = flux_hir::lower_sources(
        &mut db,
        &[
            SourceCrate {
                root: dep,
                name: Symbol::intern("original"),
                dependencies: vec![],
            },
            SourceCrate {
                root: main,
                name: Symbol::intern("consumer"),
                dependencies: vec![(Symbol::intern("renamed"), 0)],
            },
        ],
    );
    hir.diagnostics.into_iter().map(|d| d.message).collect()
}

#[test]
fn dependency_imports_keep_definition_crate_and_call_site_scope() {
    let errors = dependency_errors(
        r#"
        pub fn value() -> i32 { 42 }
        #[macro_export] macro_rules! inner { () => {{ use $crate::value as imported; imported() }}; }
        #[macro_export] macro_rules! outer { () => { $crate::inner!() }; }
        "#,
        r#"
        mod original { pub fn value() -> i32 { 99 } }
        fn main() {
            let _ = renamed::outer!();
            let _ = renamed::inner!();
            { use original::value as imported; let _ = imported(); }
        }
        "#,
    );
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn dependency_imports_cannot_escape_privacy_or_block_scope() {
    let errors = dependency_errors(
        r#"
        fn secret() {}
        pub fn public() {}
        #[macro_export] macro_rules! private { () => {{ use $crate::secret; secret(); }}; }
        #[macro_export] macro_rules! scoped { () => {{ use $crate::public as imported; imported(); }}; }
        "#,
        "fn main() { renamed::private!(); renamed::scoped!(); imported(); { imported(); } }",
    );
    assert!(
        errors.iter().any(|e| e == "function `secret` is private"),
        "{errors:?}"
    );
    assert_eq!(
        errors
            .iter()
            .filter(|e| e.contains("cannot find function `imported`"))
            .count(),
        2,
        "{errors:?}"
    );
    assert!(
        !errors.iter().any(|e| e.contains("items made by macros")),
        "{errors:?}"
    );
}

#[test]
fn later_block_items_still_shadow_dependency_macro_paths() {
    let errors = dependency_errors(
        "pub fn value() {} #[macro_export] macro_rules! import { () => { use $crate::value; }; }",
        "fn main() { renamed::import!(); mod renamed {} value(); }",
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("cannot find macro `import`")),
        "{errors:?}"
    );
}

#[test]
fn deferred_item_definitions_are_visible_only_after_the_call() {
    let (_, _, hir) = flux_hir::lower_text(
        "deferred.rs",
        r#"
        mod consumer {
            fn before() { generated!(); }
            crate::later::define!();
            fn after() { generated!(); }
        }
        mod sibling {
            fn outside() { generated!(); }
        }
        mod later {
            macro_rules! define {
                () => { macro_rules! generated { () => { () }; } };
            }
            pub(crate) use define;
        }
        "#,
    );
    let missing: Vec<_> = hir.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(missing, ["cannot find macro `generated` in this scope"; 2]);
}

#[test]
fn failed_item_expansion_does_not_hide_following_imports() {
    let (_, _, hir) = flux_hir::lower_text(
        "deferred.rs",
        r#"
        macro_rules! kept { () => { () }; }
        mod consumer {
            crate::missing!();
            pub(crate) use kept as imported;
            fn after() { kept!(); }
        }
        fn main() { consumer::imported!(); }
        "#,
    );
    let errors: Vec<_> = hir.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(errors, ["cannot find macro `missing` in this scope"]);
}

#[test]
fn selective_extern_macros_import_only_the_requested_names() {
    let dependency = r#"
        #[macro_export] macro_rules! first { () => { 41 }; }
        #[macro_export] macro_rules! second { () => { 42 }; }
        pub use first as reexported;
    "#;
    let errors = dependency_errors(
        dependency,
        r#"
        #[macro_use(first, reexported,)] extern crate renamed as _;
        fn main() { let _ = (first!(), reexported!(), second!()); }
        "#,
    );
    assert_eq!(errors, ["cannot find macro `second` in this scope"]);

    let errors = dependency_errors(
        dependency,
        "#[macro_use(missing)] extern crate renamed; fn main() {}",
    );
    assert_eq!(errors, ["imported macro `missing` not found"]);
}

#[test]
fn changed_macro_resolution_never_reuses_a_different_definition() {
    // Rustc prints "replacement". Sequential collection initially sees the
    // prelude println; reusing that expansion would silently print "original".
    let source = r#"
        mod defs {
            macro_rules! println { ($($t:tt)*) => { std::print!("replacement\n") }; }
            pub(crate) use println;
        }
        fn main() {
            println!("original");
            use defs::println;
        }
    "#;
    let (_, _, hir) = flux_hir::lower_text("late_macro_import.rs", source);
    assert!(
        hir.diagnostics.iter().any(|d| d.message
            == "body macro resolution changes after item collection are not supported yet"),
        "{:?}",
        hir.diagnostics
    );
}

#[test]
#[ignore = "needs MEMCHR_SOURCE pointing to an unchanged memchr source directory"]
fn unchanged_aarch64_defraw_imports_are_collected() {
    let path = std::path::PathBuf::from(std::env::var_os("MEMCHR_SOURCE").unwrap())
        .join("src/arch/aarch64/memchr.rs");
    let original = std::fs::read_to_string(&path).unwrap();
    let (_, _, hir) = flux_hir::lower_text(&path, &original);
    let uses: Vec<_> = hir
        .items
        .uses
        .iter()
        .filter(|u| {
            u.path
                .segments
                .first()
                .is_some_and(|n| n.as_str() == "arch")
        })
        .collect();
    assert_eq!(
        uses.len(),
        7,
        "one selected import for each unchanged defraw! call"
    );
    let expected = [
        "memchr_raw",
        "memrchr_raw",
        "memchr2_raw",
        "memrchr2_raw",
        "memchr3_raw",
        "memrchr3_raw",
        "count_raw",
    ];
    let mut functions = Vec::new();
    for import in uses {
        let block = hir.items.modules[import.module].block.as_ref().unwrap();
        let function = block.function.unwrap();
        functions.push(hir.items.functions[function].name.as_str());
        eprintln!(
            "{}: collected {:?}",
            hir.items.functions[function].name, import.path.segments
        );
    }
    assert_eq!(functions, expected);
    assert!(
        !hir.diagnostics
            .iter()
            .any(|d| d.message.contains("items made by macros"))
    );
    // Missing arch modules and debug! remain errors in this isolated file probe.
    assert!(!hir.diagnostics.is_empty());
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
}
