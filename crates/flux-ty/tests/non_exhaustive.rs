//! Definition identity and rustc comparisons across a real source-crate edge.
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use std::{fs, process::Command};

const DEP: &str = r#"
#[non_exhaustive] pub struct Named { pub value: i32 }
#[non_exhaustive] pub struct Tuple(pub i32);
#[non_exhaustive] pub struct Unit;
#[non_exhaustive] pub enum Choice { A, B(i32), C { value: i32 } }
#[non_exhaustive] pub enum Empty {}
pub type Alias = Named;
pub use Named as Renamed;
pub use Tuple as RenamedTuple;
pub use Unit as RenamedUnit;
pub use Choice as RenamedChoice;
pub fn named() -> Named { Named { value: 7 } }
pub fn tuple() -> Tuple { Tuple(8) }
pub fn unit() -> Unit { Unit }
pub fn same_crate() -> i32 {
    let Named { value } = Named { value: 1 };
    let Tuple(x) = Tuple(2);
    let Unit = Unit;
    let c = Choice::A;
    value + x + match c { Choice::A => 3, Choice::B(x) => x, Choice::C { value } => value }
}
"#;

fn errors(dep: &str, consumer: &str) -> Vec<String> {
    let mut db = SourceDb::new();
    let root = db.add("/non-exhaustive/dep.rs", dep);
    let main = db.add("/non-exhaustive/main.rs", consumer);
    let hir = flux_hir::lower_sources(
        &mut db,
        &[
            SourceCrate {
                root,
                name: Symbol::intern("dep"),
                dependencies: vec![],
            },
            SourceCrate {
                root: main,
                name: Symbol::intern("consumer"),
                dependencies: vec![(Symbol::intern("dep"), 0)],
            },
        ],
    );
    let typed = flux_ty::check_file(&hir);
    hir.diagnostics
        .iter()
        .chain(&typed.diagnostics)
        .map(|d| d.message.clone())
        .collect()
}

#[test]
fn cross_crate_matches_rustc() {
    let dir = std::env::temp_dir().join(format!("flux-non-exhaustive-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("dep.rs"), DEP).unwrap();
    let dep = Command::new("rustc")
        .current_dir(&dir)
        .args(["--edition=2024", "--crate-type=rlib", "dep.rs"])
        .output()
        .unwrap();
    assert!(
        dep.status.success(),
        "{}",
        String::from_utf8_lossy(&dep.stderr)
    );
    let cases = [
        (
            true,
            "use dep::{Tuple, Unit, RenamedTuple, RenamedUnit}; fn f(_: Tuple, _: Unit, _: RenamedTuple, _: RenamedUnit) {}",
        ),
        (true, "use dep::*; let _ = named();"),
        (false, "let dep::RenamedTuple(..) = dep::tuple();"),
        (false, "let dep::RenamedUnit = dep::unit();"),
        (
            true,
            "let dep::Named { value, .. } = dep::named(); let _ = value;",
        ),
        (
            true,
            "let dep::Tuple { 0: value, .. } = dep::tuple(); let _ = value;",
        ),
        (true, "let dep::Unit { .. } = dep::unit();"),
        (
            true,
            "let c = dep::Choice::B(3); let _ = match c { dep::Choice::A => 0, dep::Choice::B(x) => x, dep::Choice::C { value } => value, _ => 4 };",
        ),
        (
            true,
            "let c = dep::Choice::C { value: 9 }; let _ = match c { _ => 0 }; let _ = dep::same_crate();",
        ),
        (false, "let _ = dep::Named { value: 1 };"),
        (false, "let _ = dep::Alias { value: 1 };"),
        (false, "let _ = dep::Renamed { value: 1 };"),
        (false, "let _ = dep::Named { ..dep::named() };"),
        (false, "let _ = dep::Named { value: 2, ..dep::named() };"),
        (false, "let _ = dep::Tuple(1);"),
        (false, "let _ = dep::RenamedTuple(1);"),
        (false, "let _ = dep::Tuple;"),
        (false, "let _ = dep::Unit;"),
        (false, "let _ = dep::RenamedUnit;"),
        (false, "let dep::Named { value } = dep::named();"),
        (false, "let dep::Tuple { 0: value } = dep::tuple();"),
        (false, "let dep::Unit {} = dep::unit();"),
        (false, "let _ = dep::Unit {};"),
        (false, "let dep::Alias { value } = dep::named();"),
        (false, "let dep::Tuple(_) = dep::tuple();"),
        (false, "let dep::Tuple(..) = dep::tuple();"),
        (false, "let dep::Unit = dep::unit();"),
        (
            false,
            "let c = dep::Choice::A; let _ = match c { dep::Choice::A => 0, dep::Choice::B(_) => 1, dep::Choice::C { .. } => 2 };",
        ),
        (
            false,
            "let c = dep::RenamedChoice::A; let _ = match c { dep::RenamedChoice::A => 0, dep::RenamedChoice::B(_) => 1, dep::RenamedChoice::C { .. } => 2 };",
        ),
        (false, "fn f(x: dep::Empty) { match x {} }"),
        (false, "fn f(x: (dep::Empty,)) { match x {} }"),
    ];
    for (valid, body) in cases {
        let text = format!("fn main() {{ {body} }}");
        fs::write(dir.join("main.rs"), &text).unwrap();
        let rustc = Command::new("rustc")
            .current_dir(&dir)
            .args([
                "--edition=2024",
                "--emit=metadata",
                "--extern",
                "dep=libdep.rlib",
                "main.rs",
            ])
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            valid,
            "rustc: {body}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let found = errors(DEP, &text);
        assert_eq!(found.is_empty(), valid, "flux: {body}: {found:?}");
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn attribute_forms_and_placement() {
    for item in [
        "#[non_exhaustive()] struct S;",
        "#[non_exhaustive = 1] struct S;",
        "#[unsafe(non_exhaustive)] struct S;",
        "#[non_exhaustive] fn f() {}",
        "struct S { #[non_exhaustive] x: i32 }",
        "#[non_exhaustive] union U { x: i32 }",
        "enum E { #[non_exhaustive] A }",
        "enum E { #[non_exhaustive] A(i32) }",
        "enum E { #[non_exhaustive] A { x: i32 } }",
    ] {
        let found = errors(item, "fn main() {}");
        assert!(!found.is_empty(), "accepted {item}");
    }
    assert!(
        errors(
            "#[cfg_attr(all(), non_exhaustive)] pub struct S { pub x: i32 }",
            "fn main() { let _ = dep::S { x: 1 }; }"
        )
        .iter()
        .any(|e| e.contains("non-exhaustive struct"))
    );
}

#[test]
fn non_exhaustive_does_not_widen_private_constructors() {
    for item in ["struct S;", "struct S(pub i32);"] {
        let expression = if item.contains("i32") {
            "m::S(1)"
        } else {
            "m::S"
        };
        let dep = format!(
            "mod m {{ #[non_exhaustive] {item} }} pub fn test() {{ let _ = {expression}; }}"
        );
        let found = errors(&dep, "fn main() { dep::test(); }");
        assert!(found.iter().any(|e| e.contains("private")), "{found:?}");
    }
}
