//! Literal path loading keeps source bytes, edition, cfg and directory ownership.
use flux_hir::{SourceDb, lower};
use flux_intern::Symbol;
use flux_syntax::Edition;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

struct Case(PathBuf);
impl Case {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "flux-path-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn write(&self, name: &str, text: &str) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    // Compile-only oracle: even invalid Rust inputs are never executed.
    fn rustc(&self, success: bool) -> String {
        let output = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--crate-type=lib"])
            .arg(self.0.join("lib.rs"))
            .arg("-o")
            .arg(self.0.join("lib.rmeta"))
            .output()
            .unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(output.status.success(), success, "{stderr}");
        stderr
    }
    fn lower(&self) -> (SourceDb, flux_hir::Hir) {
        let mut db = SourceDb::new();
        let root = db.load(&self.0.join("lib.rs")).unwrap();
        let hir = lower(&mut db, root, Symbol::intern("paths"));
        (db, hir)
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn physical_paths_and_normal_module_directories_match_rustc() {
    let case = Case::new();
    case.write(
        "lib.rs",
        &format!(
            r##"
#[path = "chosen/custom.rs"] pub mod renamed;
mod normal;
#[path = r#"{}"#] pub mod absolute;
pub fn call() -> i32 {{ renamed::value() + normal::value() + absolute::value() }}
"##,
            case.0.join("absolute.rs").display()
        ),
    );
    case.write("chosen/custom.rs", "mod child; #[path = \"../sibling.rs\"] mod parent; pub fn value() -> i32 { child::value() + parent::value() }");
    case.write("chosen/child.rs", "pub fn value() -> i32 { 2 }");
    case.write("sibling.rs", "pub fn value() -> i32 { 3 }");
    case.write("absolute.rs", "pub fn value() -> i32 { 5 }");
    case.write("normal.rs", "mod child; #[path = \"sibling.rs\"] mod chosen; pub fn value() -> i32 { child::value() + chosen::value() }");
    case.write("normal/child.rs", "pub fn value() -> i32 { 7 }");
    case.rustc(true);
    let (db, hir) = case.lower();
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    for (_, file) in db.files().filter(|(_, f)| f.path.starts_with(&case.0)) {
        assert_eq!(file.text(), std::fs::read_to_string(&file.path).unwrap());
        assert_eq!(file.edition, Edition::E2024);
        assert_eq!(file.expansion_of, None);
    }
}

#[test]
fn conditional_paths_run_before_visible_name_allocation() {
    let case = Case::new();
    case.write(
        "lib.rs",
        r#"
#[cfg(any())] #[path="missing.rs"] mod disabled;
#[cfg_attr(all(), path="off.rs")] mod hidden;
mod hidden {}
#[cfg_attr(any(), path="missing.rs")] mod ordinary;
#[cfg_attr(all(), path="on.rs")] mod renamed;
pub fn call() -> i32 { renamed::value() }
"#,
    );
    case.write(
        "off.rs",
        "#![cfg_attr(all(), cfg(any()))]\ncompile_error!(\"disabled\");",
    );
    case.write("on.rs", "pub fn value() -> i32 { 11 }");
    case.write("ordinary.rs", "");
    case.rustc(true);
    let (_, hir) = case.lower();
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    assert_eq!(
        hir.items
            .modules
            .iter()
            .filter(|(_, m)| m.name.as_str() == "hidden")
            .count(),
        1
    );
    assert!(
        !hir.items
            .modules
            .iter()
            .any(|(_, m)| m.name.as_str() == "disabled")
    );
}

#[test]
fn unsupported_and_malformed_forms_have_explicit_diagnostics() {
    for (source, valid, expected) in [
        (
            "#[path=\"ok.rs\"] #[path=\"missing.rs\"] mod m;",
            true,
            "duplicate path attributes are not supported yet",
        ),
        (
            "#[path=\"missing.rs\"] fn f() {}",
            true,
            "the `path` attribute is not supported yet",
        ),
        (
            "#[path=\"dir\"] mod m {}",
            true,
            "path attributes on inline modules are not supported yet",
        ),
        (
            "mod outer { #[path=\"ok.rs\"] mod m; }",
            true,
            "path attributes inside inline modules or blocks are not supported yet",
        ),
        (
            "fn f() { #[path=\"ok.rs\"] mod m; }",
            true,
            "path attributes inside inline modules or blocks are not supported yet",
        ),
        (
            "#[path] mod m;",
            false,
            "expected a string literal in `path`",
        ),
        (
            "#[path(\"ok.rs\")] mod m;",
            false,
            "expected a string literal in `path`",
        ),
        (
            "#[path=12] mod m;",
            false,
            "expected a string literal in `path`",
        ),
        (
            "#[path=concat!(\"ok\", \".rs\")] mod m;",
            false,
            "expected a string literal in `path`",
        ),
        (
            "macro_rules! declare { () => { #[path=\"ok.rs\"] mod m; }; } declare!();",
            true,
            "path attributes in item macro expansions are not supported yet",
        ),
        ("#[path=\"missing.rs\"] mod m;", false, "cannot read"),
    ] {
        let case = Case::new();
        case.write("lib.rs", source);
        case.write("ok.rs", "");
        case.write("outer/ok.rs", "");
        case.rustc(valid);
        let (_, hir) = case.lower();
        assert!(
            hir.diagnostics.iter().any(|d| d.message.contains(expected)),
            "{source}: {:?}",
            hir.diagnostics
        );
    }
}

#[test]
fn recursive_paths_and_symlink_aliases_are_bounded() {
    let case = Case::new();
    case.write("lib.rs", "#[path=\"./lib.rs\"] mod again;");
    case.rustc(false);
    let (_, hir) = case.lower();
    assert_eq!(hir.diagnostics.len(), 1);
    assert!(
        hir.diagnostics[0]
            .message
            .contains("recursive module loading")
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("lib.rs", case.0.join("alias.rs")).unwrap();
        case.write("lib.rs", "#[path=\"alias.rs\"] mod again;");
        let (_, hir) = case.lower();
        assert_eq!(hir.diagnostics.len(), 1);
        assert!(
            hir.diagnostics[0]
                .message
                .contains("recursive module loading")
        );
    }
}

#[test]
fn loaded_file_diagnostics_keep_physical_source_and_privacy() {
    let case = Case::new();
    case.write(
        "lib.rs",
        "#[path=\"ok.rs\"] mod renamed; pub fn call() { renamed::private(); }",
    );
    case.write("ok.rs", "fn private() {}\nfn bad() { missing(); }");
    case.rustc(false);
    let (db, hir) = case.lower();
    assert!(
        hir.diagnostics
            .iter()
            .any(|d| d.message.contains("private"))
    );
    let missing = hir
        .diagnostics
        .iter()
        .find(|d| d.message.contains("cannot find function `missing`"))
        .unwrap();
    assert_eq!(db.file(missing.file).path, case.0.join("ok.rs"));
}

#[test]
fn path_targets_inherit_the_containing_crates_edition() {
    let case = Case::new();
    case.write(
        "lib.rs",
        "#[path=\"chosen.rs\"] mod renamed; pub fn call() { renamed::gen(); }",
    );
    case.write(
        "chosen.rs",
        "pub fn gen() {} // gen is an identifier in edition 2021\n",
    );
    let oracle = Command::new("rustc")
        .args(["--edition=2021", "--emit=metadata", "--crate-type=lib"])
        .arg(case.0.join("lib.rs"))
        .arg("-o")
        .arg(case.0.join("lib.rmeta"))
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let mut db = SourceDb::new();
    let root = db
        .load_with_edition(&case.0.join("lib.rs"), Edition::E2021)
        .unwrap();
    let hir = lower(&mut db, root, Symbol::intern("paths"));
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let target = db
        .files()
        .find(|(_, f)| f.path == case.0.join("chosen.rs"))
        .unwrap()
        .1;
    assert_eq!(target.edition, Edition::E2021);
    assert_eq!(
        target.text(),
        std::fs::read_to_string(&target.path).unwrap()
    );
}

#[test]
fn distinct_file_chains_hit_a_bounded_loading_limit() {
    let case = Case::new();
    case.write("lib.rs", "#[path=\"0.rs\"] mod next;");
    for i in 0..140 {
        case.write(
            &format!("{i}.rs"),
            &format!("#[path=\"{}.rs\"] mod next;", i + 1),
        );
    }
    let (_, hir) = case.lower();
    assert_eq!(hir.diagnostics.len(), 1);
    assert!(hir.diagnostics[0].message.contains("module nesting limit"));
}
