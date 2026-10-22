//! Source dependencies are separate crates, not modules of the executable.
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;

type Source<'a> = (&'a str, &'a str, &'a [(&'a str, usize)]);

fn check(sources: &[Source<'_>]) -> Vec<String> {
    let mut db = SourceDb::new();
    let graph: Vec<_> = sources
        .iter()
        .enumerate()
        .map(|(i, (name, text, deps))| SourceCrate {
            root: db.add(format!("/source-graph/{i}/lib.rs"), *text),
            name: Symbol::intern(name),
            dependencies: deps
                .iter()
                .map(|(name, index)| (Symbol::intern(name), *index))
                .collect(),
        })
        .collect();
    let hir = flux_hir::lower_sources(&mut db, &graph);
    let typed = flux_mir::check_file(&hir);
    let mut errors: Vec<_> = hir
        .diagnostics
        .iter()
        .chain(typed.diagnostics.iter())
        .map(|d| d.message.clone())
        .collect();
    if errors.is_empty() {
        errors.extend(
            flux_mir::build_file(&hir, &typed)
                .diagnostics
                .into_iter()
                .map(|d| d.message),
        );
    }
    errors
}

#[test]
fn dependency_aliases_reexports_and_same_identity() {
    // Repeated compilations also exercise the embedded-library cache boundary.
    for _ in 0..2 {
        let errors = check(&[
            (
                "leaf",
                "pub struct Value(pub i32); pub(crate) fn secret() -> i32 { 4 } pub fn make() -> Value { Value(secret()) }",
                &[],
            ),
            (
                "middle",
                "pub use local::Value; pub fn value() -> Value { local::make() }",
                &[("local", 0)],
            ),
            (
                "app",
                "extern crate bridge; fn take(v: alias::Value) -> i32 { v.0 } fn main() { let _ = take(bridge::value()); }",
                &[("alias", 0), ("bridge", 1)],
            ),
        ]);
        assert!(errors.is_empty(), "{errors:?}");
    }
}

#[test]
fn transitive_sibling_and_binary_crates_are_not_implicitly_visible() {
    for text in [
        "pub fn f() { leaf::make(); }",
        "pub fn f() { ::leaf::make(); }",
    ] {
        let errors = check(&[
            ("leaf", "pub fn make() {}", &[]),
            ("middle", text, &[]),
            ("app", "fn main() {}", &[("leaf", 0), ("middle", 1)]),
        ]);
        assert!(!errors.is_empty(), "sibling leaked: {text}");
    }
    let errors = check(&[
        ("leaf", "pub fn make() {}", &[]),
        (
            "middle",
            "pub fn value() { local::make(); }",
            &[("local", 0)],
        ),
        ("app", "fn main() { leaf::make(); }", &[("middle", 1)]),
    ]);
    assert!(!errors.is_empty(), "transitive dependency leaked");
    let errors = check(&[
        ("leaf", "pub fn make() { app::run(); }", &[]),
        ("app", "pub fn run() {} fn main() {}", &[("leaf", 0)]),
    ]);
    assert!(!errors.is_empty(), "binary leaked into dependency");
}

#[test]
fn dependency_privacy_and_unused_body_checks() {
    for function in ["fn secret() {}", "pub(crate) fn secret() {}"] {
        let errors = check(&[
            ("leaf", function, &[]),
            ("app", "fn main() { dep::secret(); }", &[("dep", 0)]),
        ]);
        assert!(errors.iter().any(|e| e.contains("private")), "{errors:?}");
    }
    let errors = check(&[
        ("leaf", "pub fn unused() -> i32 { let x: i32; x }", &[]),
        ("app", "fn main() {}", &[("dep", 0)]),
    ]);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("uninitialized") || e.contains("isn't initialized")),
        "{errors:?}"
    );
}

#[test]
fn dependency_macro_definition_crate_and_privacy() {
    let errors = check(&[
        (
            "leaf",
            "pub fn answer() -> i32 { 42 } #[macro_export] macro_rules! value { () => { $crate::answer() } }",
            &[],
        ),
        ("middle", "pub use local::value;", &[("local", 0)]),
        (
            "app",
            "fn main() { let _: i32 = bridge::value!(); }",
            &[("bridge", 1)],
        ),
    ]);
    assert!(errors.is_empty(), "{errors:?}");
    let errors = check(&[
        (
            "leaf",
            "fn secret() {} #[macro_export] macro_rules! access { () => { $crate::secret() } }",
            &[],
        ),
        ("app", "fn main() { dep::access!(); }", &[("dep", 0)]),
    ]);
    assert!(errors.iter().any(|e| e.contains("private")), "{errors:?}");
}

#[test]
fn distinct_crates_with_identical_library_names_remain_distinct() {
    let errors = check(&[
        ("same_name", "pub struct Value;", &[]),
        ("same_name", "pub struct Value;", &[]),
        (
            "app",
            "fn consume(_: first::Value) {} fn main() { consume(second::Value); }",
            &[("first", 0), ("second", 1)],
        ),
    ]);
    assert!(
        errors.iter().any(|e| e.contains("mismatched types")),
        "{errors:?}"
    );
}

#[test]
fn cross_crate_impl_ownership() {
    let errors = check(&[
        ("leaf", "pub struct Foreign; pub trait Trait<T> {}", &[]),
        (
            "app",
            "impl dep::Foreign { fn illegal() {} } impl dep::Trait<()> for dep::Foreign {} fn main() {}",
            &[("dep", 0)],
        ),
    ]);
    assert!(
        errors.iter().any(|e| e.contains("cannot define inherent")),
        "{errors:?}"
    );
    assert!(
        errors.iter().any(|e| e.contains("only traits defined")),
        "{errors:?}"
    );
    let errors = check(&[
        ("leaf", "pub trait Trait<T> {}", &[]),
        (
            "app",
            "struct Local; impl dep::Trait<Local> for i32 {} impl dep::Trait<()> for Box<Local> {} impl dep::Trait<()> for &Local {} fn main() {}",
            &[("dep", 0)],
        ),
    ]);
    assert!(errors.is_empty(), "{errors:?}");
    let errors = check(&[
        ("leaf", "pub trait Trait<T> {}", &[]),
        (
            "app",
            "struct Local; impl<T> dep::Trait<Local> for T {} fn main() {}",
            &[("dep", 0)],
        ),
    ]);
    assert!(
        errors.iter().any(|e| e.contains("must be covered")),
        "{errors:?}"
    );
}

#[test]
fn no_std_libraries_use_core_preludes_only() {
    let errors = check(&[
        (
            "leaf",
            "#![no_std] #[rustfmt::skip] pub fn identity<T: Copy>(value: T) -> Option<T> { Some(value) }",
            &[],
        ),
        (
            "app",
            "fn main() { assert_eq!(dep::identity(7), Some(7)); }",
            &[("dep", 0)],
        ),
    ]);
    assert!(errors.is_empty(), "{errors:?}");
    for source in [
        "pub fn f() { let _ = Vec::<i32>::new(); }",
        "pub fn f() { let _ = std::thread::panicking(); }",
        "pub fn f() { let _ = ::std::thread::panicking(); }",
        "pub fn f() { let _ = ::alloc::vec::Vec::<i32>::new(); }",
        "pub fn f() { println!(\"missing macro\"); }",
        "use core::println; pub fn f() { println!(\"not a core macro\"); }",
        "pub fn f() { let _ = vec![1]; }",
        "pub fn f() { let _ = format!(\"missing macro\"); }",
        "pub fn f() { std::println!(\"missing crate\"); }",
    ] {
        let errors = check(&[
            ("leaf", &format!("#![no_std] {source}"), &[]),
            ("app", "fn main() {}", &[("dep", 0)]),
        ]);
        assert!(!errors.is_empty(), "std prelude leaked: {source}");
    }
    let errors = check(&[
        (
            "leaf",
            "#![no_std] extern crate alloc; use alloc::vec::Vec; pub fn f() -> Vec<i32> { alloc::vec![1, 2] }",
            &[],
        ),
        (
            "app",
            "fn main() { assert_eq!(dep::f().len(), 2); }",
            &[("dep", 0)],
        ),
    ]);
    assert!(errors.is_empty(), "{errors:?}");
    let errors = check(&[
        (
            "leaf",
            "#![no_std] extern crate std; use std::println; pub fn f() { println!(\"explicit std\"); }",
            &[],
        ),
        ("app", "fn main() { dep::f(); }", &[("dep", 0)]),
    ]);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn dependency_item_macros_cannot_bypass_later_shadowing() {
    let errors = check(&[
        (
            "leaf",
            "#[macro_export] macro_rules! create { () => { fn made() {} } }",
            &[],
        ),
        (
            "app",
            "dep::create!(); mod dep {} fn main() { made(); }",
            &[("dep", 0)],
        ),
    ]);
    assert!(
        !errors.is_empty(),
        "early macro lookup ignored local module shadowing"
    );
    let errors = check(&[
        (
            "leaf",
            "#[macro_export] macro_rules! ident { ($i:ident) => { 7 }; } #[macro_export] macro_rules! invalid { () => { $crate::ident!($crate) }; }",
            &[],
        ),
        ("app", "fn main() { dep::invalid!(); }", &[("dep", 0)]),
    ]);
    assert!(!errors.is_empty(), "`$crate` matched an ident fragment");
}
