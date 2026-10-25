//! Included files inherit the source crate edition and retain separate identities.
use flux_hir::SourceDb;
use flux_intern::Symbol;
use flux_syntax::Edition;
use std::process::Command;

#[test]
fn included_source_inherits_edition_and_has_physical_diagnostics() {
    let root = std::env::temp_dir().join(format!("flux-include-edition-{}", std::process::id()));
    std::fs::create_dir_all(root.join("physical")).unwrap();
    let path = root.join("lib.rs");
    let included = root.join("physical/items.rs");
    std::fs::write(&path, "include!(\"physical/items.rs\");").unwrap();
    std::fs::write(&included, "pub fn gen() -> u32 { 23 }").unwrap();
    for (edition, text, accepts) in [(Edition::E2021, "2021", true), (Edition::E2024, "2024", false)] {
        let rustc = Command::new("rustc").arg(format!("--edition={text}"))
            .args(["--emit=metadata", "--crate-type=lib"])
            .arg(&path).arg("-o").arg(root.join("lib.rmeta")).output().unwrap();
        assert_eq!(rustc.status.success(), accepts, "{}", String::from_utf8_lossy(&rustc.stderr));
        let mut db = SourceDb::new();
        let file = db.load_with_edition(&path, edition).unwrap();
        let hir = flux_hir::lower(&mut db, file, Symbol::intern("included"));
        let mut diagnostics = db.syntax_errors();
        diagnostics.extend(hir.diagnostics);
        assert_eq!(diagnostics.is_empty(), accepts, "{diagnostics:?}");
        let (_, source) = db.files().find(|(_, source)| source.path == included).unwrap();
        assert_eq!(source.edition, edition);
        assert_eq!(source.expansion_of, None);
        for diagnostic in diagnostics {
            assert_eq!(db.file(diagnostic.file).path, included);
            assert!(diagnostic.range.end() <= source.text().len() as u32);
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
