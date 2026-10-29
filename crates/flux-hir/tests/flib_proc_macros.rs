use std::sync::Arc;

use flux_hir::flib::{CrateIndices, Decoder, Encoder, Error, IdMap, read_items, write_items};
use flux_hir::{
    CrateId, ItemTree, ProcMacro, ProcMacroExpander, ProcMacroInput, ProcMacroKind,
    ProcMacroResolver, ProcMacroToken, SourceCrate, SourceDb,
};
use flux_intern::Symbol;

#[derive(Debug)]
struct Echo {
    descriptor: Vec<u8>,
}
impl ProcMacroExpander for Echo {
    fn expand(&self, input: ProcMacroInput) -> Result<Vec<ProcMacroToken>, String> {
        if self.descriptor == b"derive" {
            Ok(Vec::new())
        } else {
            Ok(input.tokens)
        }
    }
    fn flib_descriptor(&self) -> Option<Vec<u8>> {
        Some(self.descriptor.clone())
    }
}
#[derive(Debug)]
struct Resolver;
impl ProcMacroResolver for Resolver {
    fn resolve(&self, descriptor: &[u8]) -> Result<Arc<dyn ProcMacroExpander>, String> {
        if descriptor == b"function" || descriptor == b"attribute" || descriptor == b"derive" {
            Ok(Arc::new(Echo {
                descriptor: descriptor.to_vec(),
            }))
        } else {
            Err("unknown mock host descriptor".into())
        }
    }
}
fn export(
    name: &str,
    kind: ProcMacroKind,
    helpers: Option<Vec<Symbol>>,
    descriptor: &[u8],
) -> ProcMacro {
    ProcMacro {
        name: Symbol::intern(name),
        kind,
        helpers,
        expander: Arc::new(Echo {
            descriptor: descriptor.to_vec(),
        }),
    }
}
fn encode(items: &ItemTree, krate: CrateId) -> Result<Vec<u8>, Error> {
    let mut ids = IdMap::strict();
    CrateIndices::collect(items, krate).add_mapping(&mut ids, krate, 0)?;
    for index in 0..krate.0 {
        let dep = CrateId(index);
        CrateIndices::collect(items, dep).add_mapping(&mut ids, dep, index + 1)?;
    }
    let mut e = Encoder::with_ids(ids);
    write_items(items, krate, &mut e)?;
    Ok(e.into_bytes())
}
fn decoder<'a>(
    bytes: &'a [u8],
    items: &ItemTree,
    resolver: Option<Arc<dyn ProcMacroResolver>>,
) -> Decoder<'a> {
    let mut ids = IdMap::strict();
    for index in 0..items.crates.len() {
        let dep = CrateId(index as u32);
        CrateIndices::collect(items, dep)
            .add_decode_mapping(&mut ids, dep, index as u32 + 1)
            .unwrap();
    }
    let mut d = Decoder::with_ids(bytes, ids);
    if let Some(resolver) = resolver {
        d.set_proc_macro_resolver(resolver);
    }
    d
}

#[test]
fn exported_procedural_macros_roundtrip_and_reexport_without_host_source() {
    let exports = vec![
        export(
            "identity",
            ProcMacroKind::Function,
            Some(vec![]),
            b"function",
        ),
        export("pass", ProcMacroKind::Attribute, Some(vec![]), b"attribute"),
        export(
            "Demo",
            ProcMacroKind::Derive,
            Some(vec![Symbol::intern("allowed")]),
            b"derive",
        ),
        export("UnknownHelpers", ProcMacroKind::Derive, None, b"derive"),
    ];
    let mut source_db = SourceDb::new();
    let host = source_db.add_proc_macro_crate("deleted-host.rs".into(), exports);
    let source = flux_hir::lower_sources(
        &mut source_db,
        &[SourceCrate {
            root: host,
            name: Symbol::intern("host"),
            dependencies: vec![],
        }],
    );
    assert!(source.diagnostics.is_empty(), "{:?}", source.diagnostics);
    let owner = CrateId(source.items.crates.len() as u32 - 1);
    let bytes = encode(&source.items, owner).unwrap();
    let mut db = SourceDb::new();
    let mut items = flux_hir::Core::get().items.clone();
    items.core = items.counts();
    let original_count = items.crates.len();
    assert!(
        read_items(
            &mut items,
            &mut db,
            &mut decoder(&bytes, &source.items, None)
        )
        .is_err()
    );
    assert_eq!(items.crates.len(), original_count);
    let mut d = decoder(&bytes, &items, Some(Arc::new(Resolver)));
    let (loaded, _) = read_items(&mut items, &mut db, &mut d).unwrap();
    d.finish().unwrap();
    let unknown = items.exported_macros[&(loaded, Symbol::intern("UnknownHelpers"))];
    let flux_hir::MacroKind::ProcMacro(export) = &items.macros[unknown].kind else {
        panic!("proc macro kind")
    };
    assert!(export.helpers.is_none());
    assert_eq!(export.kind, ProcMacroKind::Derive);
    let relay = db.add(
        "relay.rs",
        "pub use host::identity as forwarded; pub use host::{pass, Demo};",
    );
    let relay = flux_hir::lower_source_on(
        &mut db,
        items,
        relay,
        Symbol::intern("relay"),
        vec![(Symbol::intern("host"), loaded)],
    );
    assert!(relay.diagnostics.is_empty(), "{:?}", relay.diagnostics);
    let relay_owner = CrateId(relay.items.crates.len() as u32 - 1);
    let relay_bytes = encode(&relay.items, relay_owner).unwrap();
    let mut items = relay.items;
    let mut d = decoder(&relay_bytes, &items, Some(Arc::new(Resolver)));
    let (relay, _) = read_items(&mut items, &mut db, &mut d).unwrap();
    d.finish().unwrap();
    let root = db.add(
        "consumer.rs",
        r#"
        #[relay::pass] fn generated() -> u32 { relay::forwarded!(7u32) }
        #[derive(relay::Demo)] #[allowed] struct Kept;
        fn main() { let _ = generated(); let _ = relay::forwarded!(9u32); }
    "#,
    );
    let consumer = flux_hir::lower_source_on(
        &mut db,
        items,
        root,
        Symbol::intern("consumer"),
        vec![(Symbol::intern("relay"), relay)],
    );
    assert!(
        consumer.diagnostics.is_empty(),
        "{:?}",
        consumer.diagnostics
    );
}

#[test]
fn resolver_failure_keeps_detailed_error_and_does_not_publish_crate() {
    #[derive(Debug)]
    struct Missing;
    impl ProcMacroResolver for Missing {
        fn resolve(&self, _: &[u8]) -> Result<Arc<dyn ProcMacroExpander>, String> {
            Err("host changed; rebuild artifact".into())
        }
    }
    let mut source_db = SourceDb::new();
    let root = source_db.add_proc_macro_crate(
        "host.rs".into(),
        vec![export(
            "identity",
            ProcMacroKind::Function,
            Some(vec![]),
            b"function",
        )],
    );
    let hir = flux_hir::lower_sources(
        &mut source_db,
        &[SourceCrate {
            root,
            name: Symbol::intern("host"),
            dependencies: vec![],
        }],
    );
    let bytes = encode(&hir.items, CrateId(hir.items.crates.len() as u32 - 1)).unwrap();
    let mut db = SourceDb::new();
    let mut items = flux_hir::Core::get().items.clone();
    items.core = items.counts();
    let count = items.crates.len();
    let mut d = decoder(&bytes, &items, Some(Arc::new(Missing)));
    assert!(read_items(&mut items, &mut db, &mut d).is_err());
    assert_eq!(items.crates.len(), count);
    assert_eq!(d.proc_macro_error(), Some("host changed; rebuild artifact"));
}
