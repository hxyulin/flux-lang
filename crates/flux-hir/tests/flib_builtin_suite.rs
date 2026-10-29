//! Trusted suites preserve the complete compiler prefix without source lowering.
use flux_hir::flib::{
    Decode, Decoder, Encode, Encoder, read_builtin_suite, read_items, write_builtin_suite,
    write_items,
};
use flux_hir::{Core, CrateId, SourceCrate, SourceDb};
use flux_intern::Symbol;

#[test]
fn trusted_prefix_preserves_ids_roles_files_and_source_graph_resolution() {
    let mut db = SourceDb::new();
    db.add("unused-library-diagnostic.rs", "const UNUSED: u8 = 0;");
    let original = &Core::get().items;
    let mut encoder = Encoder::new();
    write_builtin_suite(original, &db, &mut encoder).unwrap();
    let bytes = encoder.into_bytes();
    let mut decoder = Decoder::new(&bytes);
    let (mut loaded_db, loaded) = read_builtin_suite(&mut decoder).unwrap();
    decoder.finish().unwrap();
    let mut fork = loaded_db.fork();
    let before = loaded_db.files().count();
    fork.add("fork-only.rs", "fn detached() {}");
    assert_eq!(loaded_db.files().count(), before);
    assert_eq!(fork.files().count(), before + 1);
    assert!(loaded.has_artifact_builtins());
    assert_eq!(loaded.core, original.counts());
    assert_eq!(loaded.counts(), original.counts());
    assert_eq!(loaded_db.files().count(), db.files().count());
    for ((id, a), (other, b)) in db.files().zip(loaded_db.files()) {
        assert_eq!(id, other);
        assert_eq!(a.path, b.path);
        assert_eq!(a.text(), b.text());
        assert_eq!(a.expansion_of, b.expansion_of);
        assert_eq!(a.edition, b.edition);
        assert_eq!(b.parse.root().text(), "");
    }
    assert_eq!(loaded.lang_traits, original.lang_traits);
    assert_eq!(loaded.lang_enums, original.lang_enums);
    assert_eq!(loaded.lang_fns, original.lang_fns);
    assert_eq!(loaded.panic_fn, original.panic_fn);
    assert_eq!(loaded.macro_prelude, original.macro_prelude);
    for ((id, a), (other, b)) in original.functions.iter().zip(loaded.functions.iter()) {
        assert_eq!(id, other);
        assert_eq!(a.name, b.name);
        assert_eq!(a.loc.file, b.loc.file);
        assert_eq!(a.loc.module, b.loc.module);
        assert_eq!(a.sig_range, b.sig_range);
        assert_eq!(a.intrinsic, b.intrinsic);
    }
    let mut encoder = Encoder::new();
    write_builtin_suite(&loaded, &loaded_db, &mut encoder).unwrap();
    assert_eq!(bytes, encoder.into_bytes());
    let library = loaded_db.add(
        "suite-user-library.rs",
        "pub fn message()-> &'static str { core::stringify!(ready) }",
    );
    let main = loaded_db.add(
        "suite-user-main.rs",
        "fn main(){let value=dep::message(); assert_eq!(value, \"ready\");}",
    );
    let hir = flux_hir::lower_sources_on(
        &mut loaded_db,
        loaded,
        &[
            SourceCrate {
                root: library,
                name: Symbol::intern("dep"),
                dependencies: vec![],
            },
            SourceCrate {
                root: main,
                name: Symbol::intern("user"),
                dependencies: vec![(Symbol::intern("dep"), 0)],
            },
        ],
    );
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let builtin = hir
        .items
        .functions
        .iter()
        .find(|(_, f)| f.has_body)
        .unwrap()
        .0;
    assert!(hir.body(builtin).is_none());
    assert!(
        hir.const_bodies()
            .all(|(id, _)| id.into_raw() >= hir.items.core.consts)
    );
    let source = loaded_db.add("single.rs", "fn main(){std::println!(\"suite\");}");
    let hir = flux_hir::lower_source_on(
        &mut loaded_db,
        hir.items,
        source,
        Symbol::intern("single"),
        vec![],
    );
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
}

#[test]
fn ordinary_metadata_entrypoints_reject_trusted_library_roles() {
    let db = SourceDb::new();
    let items = &Core::get().items;
    let mut ordinary = Encoder::new();
    assert!(write_items(items, CrateId(0), &mut ordinary).is_err());
    let mut trusted = Encoder::new();
    write_builtin_suite(items, &db, &mut trusted).unwrap();
    let bytes = trusted.into_bytes();
    let mut db = SourceDb::empty();
    let mut items = flux_hir::ItemTree::default();
    let mut decoder = Decoder::new(&bytes);
    assert!(read_items(&mut items, &mut db, &mut decoder).is_err());
    assert!(items.crates.is_empty());
}

#[test]
fn builtin_macro_tags_are_rejected_outside_the_suite_decoder() {
    let builtin = Core::get()
        .items
        .macros
        .iter()
        .find(|(_, definition)| matches!(definition.kind, flux_hir::MacroKind::Builtin(_)))
        .unwrap()
        .1;
    assert!(builtin.encode(&mut Encoder::new()).is_err());
    let mut e = Encoder::new();
    builtin.name.encode(&mut e).unwrap();
    builtin.krate.encode(&mut e).unwrap();
    builtin.file.encode(&mut e).unwrap();
    builtin.name_range.encode(&mut e).unwrap();
    2u8.encode(&mut e).unwrap();
    0u8.encode(&mut e).unwrap();
    assert!(flux_hir::MacroDef::decode(&mut Decoder::new(&e.into_bytes())).is_err());
}
