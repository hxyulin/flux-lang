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
fn exported_dependency_macros_are_reported_until_hygiene_is_supported() {
    let errors = check(&[
        (
            "leaf",
            "#[macro_export] macro_rules! answer { () => { 42 } }",
            &[],
        ),
        ("app", "fn main() {}", &[("dep", 0)]),
    ]);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("exported dependency macros are not supported")),
        "{errors:?}"
    );
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
