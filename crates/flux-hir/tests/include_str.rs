//! Literal include_str data uses physical source identity, not module names.
use flux_hir::{Expr, Literal, SourceCrate, SourceDb};
use flux_intern::Symbol;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

struct Case(PathBuf);
impl Case {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "flux-include-str-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    fn rustc(&self, valid: bool) {
        let result = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--crate-type=lib"])
            .arg(self.0.join("lib.rs"))
            .arg("-o")
            .arg(self.0.join("lib.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            result.status.success(),
            valid,
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    // Both sides of the dependency graph are compile-only rustc oracles.
    fn rustc_dependency(&self, dependency: &str) {
        let metadata = self.0.join("libdep.rmeta");
        let dep = Command::new("rustc")
            .args([
                "--edition=2024",
                "--emit=metadata",
                "--crate-type=lib",
                "--crate-name=dep",
            ])
            .arg(self.0.join(dependency))
            .arg("-o")
            .arg(&metadata)
            .output()
            .unwrap();
        assert!(
            dep.status.success(),
            "{}",
            String::from_utf8_lossy(&dep.stderr)
        );
        let main = Command::new("rustc")
            .args([
                "--edition=2024",
                "--emit=metadata",
                "--crate-type=lib",
                "--extern",
            ])
            .arg(format!("dep={}", metadata.display()))
            .arg(self.0.join("lib.rs"))
            .arg("-o")
            .arg(self.0.join("main.rmeta"))
            .output()
            .unwrap();
        assert!(
            main.status.success(),
            "{}",
            String::from_utf8_lossy(&main.stderr)
        );
    }
    fn lower(&self) -> (SourceDb, flux_hir::Hir) {
        let mut db = SourceDb::new();
        let root = db.load(&self.0.join("lib.rs")).unwrap();
        let hir = flux_hir::lower(&mut db, root, Symbol::intern("includes"));
        (db, hir)
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn strings(hir: &flux_hir::Hir) -> Vec<&str> {
    hir.bodies
        .iter()
        .flat_map(|(_, (body, _))| body.exprs.iter())
        .filter_map(|(_, expr)| match expr {
            Expr::Literal(Literal::Str(value)) => Some(&**value),
            _ => None,
        })
        .collect()
}

#[test]
fn physical_nested_and_dependency_files_preserve_utf8_data() {
    let case = Case::new();
    let contents = "\u{feff}α\r\n\"quote\" \\ slash\0終\n";
    case.write("lib.rs", "#[path=\"physical/renamed.rs\"] mod logical; pub fn root() -> &'static str { include_str!(r#\"empty\"#, ) }");
    case.write("empty", "");
    case.write(
        "physical/renamed.rs",
        "mod inline { pub fn text() -> &'static str { include_str!(\"data\") } }",
    );
    case.write("physical/data", contents);
    case.rustc(true);
    let (_, hir) = case.lower();
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let values = strings(&hir);
    assert!(values.contains(&contents));
    assert!(values.contains(&""));

    case.write(
        "dependency/lib.rs",
        "pub fn text() -> &'static str { include_str!(\"data\") }",
    );
    case.write("dependency/data", "dependency bytes");
    case.write("lib.rs", "pub fn root() -> &'static str { dep::text() }");
    case.rustc_dependency("dependency/lib.rs");
    let mut db = SourceDb::new();
    let dep = db.load(&case.0.join("dependency/lib.rs")).unwrap();
    let root = db.load(&case.0.join("lib.rs")).unwrap();
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
                name: Symbol::intern("main"),
                dependencies: vec![(Symbol::intern("dep"), 0)],
            },
        ],
    );
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    assert!(strings(&hir).contains(&"dependency bytes"));
}

#[test]
fn cfg_disabled_calls_do_not_load_missing_files() {
    let case = Case::new();
    case.write(
        "lib.rs",
        r#"
#[cfg(any())] const NO: &str = include_str!("absent");
#[cfg(any())] fn no() { include_str!("absent"); }
pub fn yes() { #[cfg(any())] { include_str!("absent"); } }
"#,
    );
    case.rustc(true);
    let (_, hir) = case.lower();
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
}

#[test]
fn invalid_inputs_are_diagnosed_compile_only() {
    let case = Case::new();
    case.write("invalid", [0xff, 0xfe]);
    for (argument, expected) in [
        ("\"absent\"", "cannot read"),
        ("\"invalid\"", "cannot read"),
        ("", "requires a string literal filename"),
        ("42", "requires a string literal filename"),
        ("b\"invalid\"", "requires a string literal filename"),
        (
            "\"invalid\", \"invalid\"",
            "requires a string literal filename",
        ),
        ("\"\\q\"", "unknown character escape"),
    ] {
        case.write(
            "lib.rs",
            format!("pub fn text() -> &'static str {{ include_str!({argument}) }}"),
        );
        case.rustc(false);
        let (db, hir) = case.lower();
        let diagnostic = hir
            .diagnostics
            .iter()
            .find(|d| d.message.contains(expected))
            .unwrap_or_else(|| panic!("{argument}: {:?}", hir.diagnostics));
        assert_eq!(db.file(diagnostic.file).path, case.0.join("lib.rs"));
        assert!(diagnostic.range.end() as usize <= db.file(diagnostic.file).text().len());
    }
}

#[test]
fn expanded_calls_fail_explicitly_instead_of_guessing_a_root() {
    let case = Case::new();
    case.write("data", "root data");
    case.write("defs/data", "definition data");
    case.write(
        "defs/macros.rs",
        "#[macro_export] macro_rules! other { () => { include_str!(\"data\") }; }",
    );
    for source in [
        "macro_rules! wrapped { () => { include_str!(\"data\") }; } pub fn f() -> &'static str { wrapped!() }",
        "macro_rules! forward { ($s:literal) => { include_str!($s) }; } pub fn f() -> &'static str { forward!(\"data\") }",
        "macro_rules! item { () => { pub const TEXT: &str = include_str!(\"data\"); }; } item!();",
        "#[path=\"defs/macros.rs\"] mod defs; pub fn f() -> &'static str { other!() }",
    ] {
        case.write("lib.rs", source);
        case.rustc(true);
        let (db, hir) = case.lower();
        // The existing expansion record retains its invocation file, while
        // definition tokens have no physical FileId of their own yet.
        assert!(
            db.files()
                .any(|(_, file)| file.path == case.0.join("lib.rs") && file.expansion_of.is_some())
        );
        assert!(
            hir.diagnostics
                .iter()
                .any(|d| d.message == "include_str! in macro expansions is not supported yet"),
            "{:?}",
            hir.diagnostics
        );
    }
    case.write(
        "lib.rs",
        "pub fn f() -> &'static str { include_str!(concat!(\"da\", \"ta\")) }",
    );
    case.rustc(true);
    let (_, hir) = case.lower();
    assert!(
        hir.diagnostics
            .iter()
            .any(|d| d.message.contains("requires a string literal filename"))
    );
}

#[test]
fn decoded_literal_paths_support_parent_absolute_and_escaped_names() {
    let case = Case::new();
    case.write("data", "parent bytes");
    case.write("physical/quote\"終", "escaped filename bytes");
    case.write("lib.rs", "#[path=\"physical/module.rs\"] mod child;");
    case.write(
        "physical/module.rs",
        format!(
            r##"
pub fn parent() -> &'static str {{ include_str!("../data") }}
pub fn absolute() -> &'static str {{ include_str!({:?}) }}
pub fn escaped() -> &'static str {{ include_str!("quote\"終") }}
"##,
            case.0.join("data").to_str().unwrap()
        ),
    );
    case.rustc(true);
    let (_, hir) = case.lower();
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let values = strings(&hir);
    assert_eq!(values.iter().filter(|&&v| v == "parent bytes").count(), 2);
    assert!(values.contains(&"escaped filename bytes"));
}

#[test]
fn dependency_macro_origin_is_rejected_explicitly() {
    let case = Case::new();
    case.write(
        "dep/lib.rs",
        "#[macro_export] macro_rules! text { () => { include_str!(\"data\") }; }",
    );
    case.write("dep/data", "definition bytes");
    case.write("data", "invocation bytes");
    case.write("lib.rs", "pub fn text() -> &'static str { dep::text!() }");
    case.rustc_dependency("dep/lib.rs");
    let mut db = SourceDb::new();
    let dep = db.load(&case.0.join("dep/lib.rs")).unwrap();
    let root = db.load(&case.0.join("lib.rs")).unwrap();
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
                name: Symbol::intern("main"),
                dependencies: vec![(Symbol::intern("dep"), 0)],
            },
        ],
    );
    assert!(
        hir.diagnostics
            .iter()
            .any(|d| d.message == "include_str! in macro expansions is not supported yet"),
        "{:?}",
        hir.diagnostics
    );
}
