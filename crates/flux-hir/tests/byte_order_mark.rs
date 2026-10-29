//! A leading byte order mark is not part of a loaded file's text.
use flux_hir::{SourceDb, lower};
use flux_intern::Symbol;

#[test]
fn loaded_files_drop_a_leading_byte_order_mark() {
    let dir = std::env::temp_dir().join(format!("flux-bom-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("main.rs"),
        "\u{feff}mod m;\nfn main() { m::f(); missing(); }",
    )
    .unwrap();
    std::fs::write(dir.join("m.rs"), "\u{feff}pub fn f() {}").unwrap();
    let mut db = SourceDb::new();
    let root = db.load(&dir.join("main.rs")).unwrap();
    let hir = lower(&mut db, root, Symbol::intern("bom"));
    std::fs::remove_dir_all(dir).unwrap();
    assert!(db.syntax_errors().is_empty(), "{:?}", db.syntax_errors());
    let [d] = &hir.diagnostics[..] else {
        panic!("{:?}", hir.diagnostics);
    };
    assert_eq!(d.message, "cannot find function `missing` in this scope");
    let text = db.file(d.file).text();
    assert_eq!(
        &text[d.range.start() as usize..d.range.end() as usize],
        "missing"
    );
}
