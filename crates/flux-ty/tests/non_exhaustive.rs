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
    ] {
        let found = errors(item, "fn main() {}");
        assert!(!found.is_empty(), "accepted {item}");
    }
    for item in [
        "enum E { #[non_exhaustive] A }",
        "enum E { #[non_exhaustive] A(i32) }",
        "enum E { #[non_exhaustive] A { x: i32 } }",
    ] {
        assert!(errors(item, "fn main() {}").is_empty(), "rejected {item}");
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

#[test]
fn variant_contracts_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-open-variants-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let dep = r#"
pub enum E { #[non_exhaustive] Unit, #[non_exhaustive] Tuple(u8), #[non_exhaustive] Record{v:u8}, Other }
pub enum Closed { #[non_exhaustive] R{x:bool}, Other }
pub use E::{Unit as RenamedUnit, Tuple as RenamedTuple, Record as RenamedRecord};
pub type Alias = E;
pub fn make()->E { E::Record{v:7} }
pub fn closed()->Closed { Closed::Other }
pub fn local()->u8 { match E::Tuple(7) { E::Tuple(x)=>x,E::Unit=>0,E::Record{v}=>v,E::Other=>0 } }
"#;
    fs::write(root.join("dep.rs"), dep).unwrap();
    let output = Command::new("rustc")
        .current_dir(&root)
        .args(["--edition=2024", "--crate-type=rlib", "dep.rs"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for (name, source) in [
        ("unit_type_import", "use dep::E::Unit as _;fn main(){}"),
        ("tuple_type_import", "use dep::E::Tuple as _;fn main(){}"),
        (
            "unit_record_pattern",
            "fn main(){match dep::make(){dep::E::Unit{..}=>{},_=>{}}}",
        ),
        (
            "tuple_record_pattern",
            "fn main(){match dep::make(){dep::E::Tuple{0:x,..}=>{let _=x;},_=>{}}}",
        ),
        (
            "tuple_record_exact",
            "fn main(){match dep::make(){dep::E::Tuple{0:x}=>{let _=x;},_=>{}}}",
        ),
        (
            "unit_record_exact",
            "fn main(){match dep::make(){dep::E::Unit{}=>{},_=>{}}}",
        ),
        (
            "record_rest",
            "fn main(){match dep::make(){dep::E::Record{v,..}=>assert_eq!(v,7),_=>{}}}",
        ),
        (
            "record_alias",
            "fn main(){match dep::make(){dep::Alias::Record{v,..}=>assert_eq!(v,7),_=>{}}}",
        ),
        (
            "record_reexport",
            "fn main(){match dep::make(){dep::RenamedRecord{v,..}=>assert_eq!(v,7),_=>{}}}",
        ),
        (
            "closed_exhaustive",
            "fn main(){match dep::closed(){dep::Closed::R{x:true,..}=>{},dep::Closed::R{x:false,..}=>{},dep::Closed::Other=>{}}}",
        ),
        ("unit_value", "fn main(){let _=dep::E::Unit;}"),
        (
            "unit_pattern",
            "fn main(){match dep::make(){dep::E::Unit=>{},_=>{}}}",
        ),
        ("tuple_value", "fn main(){let _=dep::E::Tuple(1);}"),
        (
            "tuple_pattern",
            "fn main(){match dep::make(){dep::E::Tuple(..)=>{},_=>{}}}",
        ),
        ("tuple_alias", "fn main(){let _=dep::Alias::Tuple(1);}"),
        ("tuple_reexport", "fn main(){let _=dep::RenamedTuple(1);}"),
        ("unit_reexport", "fn main(){let _=dep::RenamedUnit;}"),
        ("record_value", "fn main(){let _=dep::E::Record{v:1};}"),
        (
            "record_alias_value",
            "fn main(){let _=dep::Alias::Record{v:1};}",
        ),
        (
            "record_reexport_value",
            "fn main(){let _=dep::RenamedRecord{v:1};}",
        ),
        (
            "record_exact",
            "fn main(){match dep::make(){dep::E::Record{v}=>{let _=v;},_=>{}}}",
        ),
        (
            "record_alias_exact",
            "fn main(){match dep::make(){dep::Alias::Record{v}=>{let _=v;},_=>{}}}",
        ),
        ("glob_unit", "use dep::E::*;fn main(){let _=Unit;}"),
        (
            "glob_record",
            "use dep::E::*;fn main(){match dep::make(){Record{v,..}=>{let _=v;},_=>{}}}",
        ),
    ] {
        fs::write(root.join("main.rs"), source).unwrap();
        let reference = Command::new("rustc")
            .current_dir(&root)
            .args([
                "--edition=2024",
                "--extern",
                "dep=libdep.rlib",
                "main.rs",
                "-o",
                "program",
            ])
            .output()
            .unwrap();
        let diagnostics = errors(dep, source);
        assert_eq!(
            diagnostics.is_empty(),
            reference.status.success(),
            "{name}: rustc={} Flux={diagnostics:?}",
            String::from_utf8_lossy(&reference.stderr)
        );
    }
    for source in [
        "#[derive(Default)]enum E{#[default]#[non_exhaustive]Unit}fn main(){}",
        "enum E{#[non_exhaustive(bad)]V}fn main(){}",
        "enum E{#[non_exhaustive=1]V}fn main(){}",
    ] {
        let diagnostics = errors("", source);
        assert!(!diagnostics.is_empty(), "accepted {source}");
    }
    fs::remove_dir_all(root).unwrap();
}
