use flux_hir::flib::{CrateIndices, Decoder, Encoder, IdMap, read_items, write_items};
use flux_hir::{CrateId, ItemTree, SourceDb};
use flux_intern::Symbol;

fn encode(items: &ItemTree, krate: CrateId) -> Vec<u8> {
    let mut ids = IdMap::strict();
    CrateIndices::collect(items, krate)
        .add_mapping(&mut ids, krate, 0)
        .unwrap();
    for index in 0..krate.0 {
        let dep = CrateId(index);
        CrateIndices::collect(items, dep)
            .add_mapping(&mut ids, dep, index + 1)
            .unwrap();
    }
    let mut encoder = Encoder::with_ids(ids);
    write_items(items, krate, &mut encoder).unwrap();
    encoder.into_bytes()
}

#[test]
fn append_items_and_expand_exported_macro_without_dependency_syntax() {
    let mut original = SourceDb::new();
    let a = original.add(
        "a.rs",
        r#"
        mod inner { pub struct Record<T> { pub value: T } }
        pub use inner::Record;
        pub trait Fetch { type Output; fn fetch(&self) -> Self::Output; }
        impl Fetch for Record<u32> { type Output = u32; fn fetch(&self) -> u32 { self.value } }
        pub const COUNT: usize = 4;
        pub fn identity<T>(x: T) -> T { x }
        #[macro_export] macro_rules! make { ($value:expr) => { $crate::Record { value: $value } }; }
    "#,
    );
    let source = flux_hir::lower(&mut original, a, Symbol::intern("a"));
    assert!(source.diagnostics.is_empty(), "{:?}", source.diagnostics);
    let owner = CrateId(source.items.crates.len() as u32 - 1);
    let bytes = encode(&source.items, owner);
    assert_eq!(
        bytes,
        encode(&source.items, owner),
        "metadata is deterministic"
    );

    let mut db = SourceDb::new();
    let noise = db.add("noise.rs", "pub fn offset() {} struct Extra;");
    let mut items = flux_hir::lower(&mut db, noise, Symbol::intern("noise")).items;
    let mut ids = IdMap::strict();
    for index in 0..owner.0 {
        let dep = CrateId(index);
        CrateIndices::collect(&items, dep)
            .add_decode_mapping(&mut ids, dep, index + 1)
            .unwrap();
    }
    let mut decoder = Decoder::with_ids(&bytes, ids);
    let (loaded, indices) = read_items(&mut items, &mut db, &mut decoder).unwrap();
    decoder.finish().unwrap();
    assert!(!indices.functions.is_empty());
    for index in indices.files {
        assert_eq!(
            db.file(flux_hir::FileId(index)).parse.tree().text(),
            "",
            "dependency source is diagnostic-only"
        );
    }
    let b = db.add(
        "b.rs",
        "fn main() { let value = renamed::make!(42u32); let _ = renamed::identity(value); }",
    );
    let hir = flux_hir::lower_source_on(
        &mut db,
        items,
        b,
        Symbol::intern("b"),
        vec![(Symbol::intern("renamed"), loaded)],
    );
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    assert!(
        hir.bodies
            .iter()
            .all(|(id, _)| hir.items.modules[hir.items.functions[id].loc.module].krate != loaded)
    );
}

#[test]
fn truncated_item_records_return_errors() {
    let mut db = SourceDb::new();
    let root = db.add("tiny.rs", "pub struct Tiny; pub fn value() -> u32 { 7 }");
    let hir = flux_hir::lower(&mut db, root, Symbol::intern("tiny"));
    let owner = CrateId(hir.items.crates.len() as u32 - 1);
    let bytes = encode(&hir.items, owner);
    for end in [0, 1, 20, bytes.len() / 2, bytes.len() - 1] {
        let mut db = SourceDb::new();
        let mut items = flux_hir::Core::get().items.clone();
        items.core = items.counts();
        let mut ids = IdMap::strict();
        for index in 0..owner.0 {
            let dep = CrateId(index);
            CrateIndices::collect(&items, dep)
                .add_decode_mapping(&mut ids, dep, index + 1)
                .unwrap();
        }
        assert!(
            read_items(
                &mut items,
                &mut db,
                &mut Decoder::with_ids(&bytes[..end], ids)
            )
            .is_err(),
            "prefix {end}"
        );
    }
}

fn imported(source: &str) -> (SourceDb, ItemTree, CrateId) {
    imported_edition(source, flux_syntax::Edition::E2024)
}

fn imported_edition(source: &str, edition: flux_syntax::Edition) -> (SourceDb, ItemTree, CrateId) {
    let mut db = SourceDb::new();
    let root = db.add_with_edition("dependency.rs", source, edition);
    let hir = flux_hir::lower(&mut db, root, Symbol::intern("dependency"));
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let owner = CrateId(hir.items.crates.len() as u32 - 1);
    let bytes = encode(&hir.items, owner);
    let mut db = SourceDb::new();
    let mut items = flux_hir::Core::get().items.clone();
    items.core = items.counts();
    let mut ids = IdMap::strict();
    for index in 0..owner.0 {
        let dep = CrateId(index);
        CrateIndices::collect(&items, dep)
            .add_decode_mapping(&mut ids, dep, index + 1)
            .unwrap();
    }
    let mut decoder = Decoder::with_ids(&bytes, ids);
    let (loaded, _) = read_items(&mut items, &mut db, &mut decoder).unwrap();
    decoder.finish().unwrap();
    (db, items, loaded)
}

#[test]
fn imported_name_resolution_matches_source_graph_for_aliases_and_privacy() {
    let dependency = r#"
        mod hidden { pub struct Value; pub fn helper() {} }
        pub use hidden::{Value, helper};
        pub(crate) fn restricted() {}
        #[macro_export] macro_rules! create { () => { $crate::Value }; }
        #[macro_export] macro_rules! hygienic { ($value:expr) => {{ let temporary = $value; temporary }}; }
    "#;
    let consumers = [
        "fn main() { let _ = renamed::Value; renamed::helper(); }",
        "fn main() { let temporary = 4u32; let _ = renamed::hygienic!(temporary); let _ = renamed::create!(); }",
        "fn main() { let _ = renamed::hidden::Value; }",
        "fn main() { renamed::restricted(); }",
        "fn main() { let _ = undeclared::Value; }",
    ];
    for consumer in consumers {
        let (mut db, items, loaded) = imported(dependency);
        let root = db.add("consumer.rs", consumer);
        let actual = flux_hir::lower_source_on(
            &mut db,
            items,
            root,
            Symbol::intern("consumer"),
            vec![(Symbol::intern("renamed"), loaded)],
        );
        let mut baseline = SourceDb::new();
        let dep = baseline.add("dependency.rs", dependency);
        let root = baseline.add("consumer.rs", consumer);
        let expected = flux_hir::lower_sources(
            &mut baseline,
            &[
                flux_hir::SourceCrate {
                    root: dep,
                    name: Symbol::intern("dependency"),
                    dependencies: vec![],
                },
                flux_hir::SourceCrate {
                    root,
                    name: Symbol::intern("consumer"),
                    dependencies: vec![(Symbol::intern("renamed"), 0)],
                },
            ],
        );
        let diagnostics = |hir: &flux_hir::Hir| {
            hir.diagnostics
                .iter()
                .map(|d| (d.message.clone(), d.range))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            diagnostics(&actual),
            diagnostics(&expected),
            "consumer: {consumer}"
        );
    }
}

#[test]
fn crate_configuration_survives_metadata_roundtrip() {
    let mut db = SourceDb::new();
    let root = db.add(
        "configured.rs",
        "#[cfg(feature = \"enabled\")] pub struct Enabled; pub fn function() {}",
    );
    db.set_crate_cfg(
        root,
        flux_hir::CrateCfg {
            names: vec![Symbol::intern("custom")],
            values: vec![(Symbol::intern("feature"), Symbol::intern("enabled"))],
        },
    )
    .unwrap();
    let mut environment = std::collections::BTreeMap::new();
    environment.insert("CARGO_PKG_NAME".to_owned(), "configured".to_owned());
    db.set_crate_env(root, environment).unwrap();
    db.set_crate_directory(root, std::env::current_dir().unwrap())
        .unwrap();
    let hir = flux_hir::lower(&mut db, root, Symbol::intern("configured"));
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let owner = CrateId(hir.items.crates.len() as u32 - 1);
    let bytes = encode(&hir.items, owner);
    let mut db = SourceDb::new();
    let mut items = flux_hir::Core::get().items.clone();
    items.core = items.counts();
    let mut ids = IdMap::strict();
    for index in 0..owner.0 {
        let dep = CrateId(index);
        CrateIndices::collect(&items, dep)
            .add_decode_mapping(&mut ids, dep, index + 1)
            .unwrap();
    }
    let mut decoder = Decoder::with_ids(&bytes, ids);
    let (loaded, _) = read_items(&mut items, &mut db, &mut decoder).unwrap();
    decoder.finish().unwrap();
    let before = &hir.items.crates[owner.0 as usize];
    let after = &items.crates[loaded.0 as usize];
    assert_eq!(after.cfg, before.cfg);
    assert_eq!(after.environment, before.environment);
    assert_eq!(after.compilation_dir, before.compilation_dir);
    assert_eq!(
        encode(&items, loaded),
        bytes,
        "loaded metadata can be repacked without source semantics"
    );
}

#[test]
fn body_only_included_sources_keep_their_local_file_indices_after_loading() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "flux-hir-flib-include-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(directory.join("fragment.rs"), "42u32").unwrap();
    let mut db = SourceDb::new();
    let root = db.add(
        directory.join("lib.rs"),
        "pub fn value() -> u32 { include!(\"fragment.rs\") }",
    );
    let hir = flux_hir::lower(&mut db, root, Symbol::intern("included"));
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let owner = CrateId(hir.items.crates.len() as u32 - 1);
    let original = CrateIndices::collect(&hir.items, owner);
    assert!(original.files.len() > 1);
    let bytes = encode(&hir.items, owner);
    std::fs::remove_dir_all(&directory).unwrap();
    let mut db = SourceDb::new();
    let mut items = flux_hir::Core::get().items.clone();
    items.core = items.counts();
    let mut ids = IdMap::strict();
    for index in 0..owner.0 {
        let dep = CrateId(index);
        CrateIndices::collect(&items, dep)
            .add_decode_mapping(&mut ids, dep, index + 1)
            .unwrap();
    }
    let mut previous_base = None;
    for _ in 0..2 {
        let mut decoder = Decoder::with_ids(&bytes, ids.clone());
        let (loaded, indices) = read_items(&mut items, &mut db, &mut decoder).unwrap();
        decoder.finish().unwrap();
        assert_eq!(CrateIndices::collect(&items, loaded).files, indices.files);
        let base = indices.source_ranges[0].1;
        assert_ne!(
            Some(base),
            previous_base,
            "independent artifact includes need distinct process coordinates"
        );
        previous_base = Some(base);
        assert_eq!(
            encode(&items, loaded),
            bytes,
            "wire spans must remain canonical after remapping"
        );
    }
}

#[test]
fn included_spans_encode_file_identity_and_decode_fresh_offsets() {
    use flux_hir::flib::{Decode, Encode, IdKind};
    let mut encoded_ids = IdMap::strict();
    encoded_ids.insert(IdKind::File, 7, 0);
    encoded_ids.add_source_range(7, 1 << 30, (1 << 30) + 100);
    let mut encoder = Encoder::with_ids(encoded_ids);
    flux_syntax::TextRange::new((1 << 30) + 3, (1 << 30) + 8)
        .encode(&mut encoder)
        .unwrap();
    let bytes = encoder.into_bytes();
    let mut decoded_ids = IdMap::strict();
    decoded_ids.insert(IdKind::File, 0, 42);
    decoded_ids.add_source_range(42, (1 << 30) + 200, (1 << 30) + 300);
    let mut decoder = Decoder::with_ids(&bytes, decoded_ids);
    let range = flux_syntax::TextRange::decode(&mut decoder).unwrap();
    decoder.finish().unwrap();
    assert_eq!(
        range,
        flux_syntax::TextRange::new((1 << 30) + 203, (1 << 30) + 208)
    );
}

#[test]
fn actual_include_and_environment_inputs_are_retained_in_metadata() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "flux-hir-flib-inputs-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).unwrap();
    let text_path = directory.join("message.txt");
    let bytes_path = directory.join("data.bin");
    std::fs::write(&text_path, b"hello\r\n").unwrap();
    std::fs::write(&bytes_path, [0, 255, 7]).unwrap();
    let mut db = SourceDb::new();
    let root = db.add(
        directory.join("lib.rs"),
        r#"
        pub const MESSAGE: &str = include_str!("message.txt");
        pub const BYTES: &[u8] = include_bytes!("data.bin");
        pub const NAME: &str = env!("NAME");
        pub const ABSENT: Option<&str> = option_env!("ABSENT");
    "#,
    );
    let mut environment = std::collections::BTreeMap::new();
    environment.insert("NAME".to_owned(), "configured".to_owned());
    db.set_crate_env(root, environment).unwrap();
    let hir = flux_hir::lower(&mut db, root, Symbol::intern("inputs"));
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let owner = CrateId(hir.items.crates.len() as u32 - 1);
    let inputs = hir.items.crates[owner.0 as usize].tracked_inputs();
    assert_eq!(
        inputs.files.get(&text_path),
        Some(&flux_hir::flib::digest(b"hello\r\n"))
    );
    assert_eq!(
        inputs.files.get(&bytes_path),
        Some(&flux_hir::flib::digest(&[0, 255, 7]))
    );
    assert_eq!(
        inputs.environment.get("NAME"),
        Some(&Some("configured".to_owned()))
    );
    assert_eq!(inputs.environment.get("ABSENT"), Some(&None));
    let bytes = encode(&hir.items, owner);
    std::fs::remove_dir_all(&directory).unwrap();
    let mut db = SourceDb::new();
    let mut items = flux_hir::Core::get().items.clone();
    items.core = items.counts();
    let mut ids = IdMap::strict();
    for index in 0..owner.0 {
        let dep = CrateId(index);
        CrateIndices::collect(&items, dep)
            .add_decode_mapping(&mut ids, dep, index + 1)
            .unwrap();
    }
    let mut decoder = Decoder::with_ids(&bytes, ids);
    let (loaded, _) = read_items(&mut items, &mut db, &mut decoder).unwrap();
    decoder.finish().unwrap();
    assert_eq!(items.crates[loaded.0 as usize].tracked_inputs(), inputs);
}

#[test]
fn record_limits_are_symmetric_for_encoding_and_decoding() {
    use flux_hir::flib::{Decode, Encode, MAX_RECORD_LENGTH};
    let value = "x".repeat(MAX_RECORD_LENGTH);
    let mut encoder = Encoder::new();
    value.encode(&mut encoder).unwrap();
    let bytes = encoder.into_bytes();
    let mut decoder = Decoder::new(&bytes);
    assert_eq!(String::decode(&mut decoder).unwrap(), value);
    decoder.finish().unwrap();
    assert!((value + "x").encode(&mut Encoder::new()).is_err());
    let length = (MAX_RECORD_LENGTH as u32 + 1).to_le_bytes();
    assert!(String::decode(&mut Decoder::new(&length)).is_err());
}

#[test]
fn hash_maps_encode_mapped_keys_and_keep_the_encoder_map() {
    use flux_hir::flib::{Decode, Encode, IdKind};
    use std::collections::HashMap;
    let mut ids = IdMap::strict();
    ids.insert(IdKind::Crate, 7, 0);
    ids.insert(IdKind::Crate, 5, 1);
    let map: HashMap<CrateId, u32> = [(CrateId(5), 50), (CrateId(7), 70)].into();
    let mut encoder = Encoder::with_ids(ids);
    map.encode(&mut encoder).unwrap();
    // The encoder still resolves references after encoding the keys.
    CrateId(5).encode(&mut encoder).unwrap();
    let unmapped: HashMap<CrateId, u32> = [(CrateId(9), 90)].into();
    assert!(unmapped.encode(&mut encoder).is_err());
    assert!(encoder.ids.get(IdKind::Crate, 7).is_ok());
    let bytes = encoder.into_bytes();
    // Entries are sorted by their encoded keys, so artifact indices 0 then 1.
    assert_eq!(&bytes[..4], &2u32.to_le_bytes());
    let mut back = IdMap::strict();
    back.insert(IdKind::Crate, 0, 7);
    back.insert(IdKind::Crate, 1, 5);
    let mut decoder = Decoder::with_ids(&bytes, back);
    assert_eq!(HashMap::<CrateId, u32>::decode(&mut decoder).unwrap(), map);
    assert_eq!(CrateId::decode(&mut decoder).unwrap(), CrateId(5));
}

#[test]
fn artifact_root_cannot_borrow_embedded_std_file_privileges() {
    let mut db = SourceDb::new();
    let root = db.add("forged.rs", "pub fn value() {}");
    let mut hir = flux_hir::lower(&mut db, root, Symbol::intern("forged"));
    let owner = CrateId(hir.items.crates.len() as u32 - 1);
    let module = hir.items.crates[owner.0 as usize].root;
    hir.items.modules[module].file = flux_hir::FileId::std_root();
    let bytes = encode(&hir.items, owner);
    let mut db = SourceDb::new();
    let mut items = flux_hir::Core::get().items.clone();
    items.core = items.counts();
    let count = items.crates.len();
    let mut ids = IdMap::strict();
    for index in 0..owner.0 {
        let dep = CrateId(index);
        CrateIndices::collect(&items, dep)
            .add_decode_mapping(&mut ids, dep, index + 1)
            .unwrap();
    }
    let error = read_items(&mut items, &mut db, &mut Decoder::with_ids(&bytes, ids)).unwrap_err();
    assert_eq!(error.0, "invalid artifact root ownership");
    assert_eq!(items.crates.len(), count);
}

#[test]
fn exported_macro_keeps_its_definition_edition() {
    let definition = r#"
        #[macro_export] macro_rules! choose {
            ($expression:expr) => { missing_value };
            (const $block:block) => { 0u32 };
        }
    "#;
    for (edition, expect_errors) in [
        (flux_syntax::Edition::E2021, false),
        (flux_syntax::Edition::E2024, true),
    ] {
        let (mut db, items, owner) = imported_edition(definition, edition);
        let root = db.add(
            "consumer.rs",
            "fn main() { let _ = dep::choose!(const { 1u32 }); }",
        );
        let hir = flux_hir::lower_source_on(
            &mut db,
            items,
            root,
            Symbol::intern("consumer"),
            vec![(Symbol::intern("dep"), owner)],
        );
        assert_eq!(
            !hir.diagnostics.is_empty(),
            expect_errors,
            "edition {edition:?}: {:?}",
            hir.diagnostics
        );
    }
}

#[test]
fn exported_macro_uses_the_invocation_crates_cfg() {
    let definition = r#"
        #[macro_export] macro_rules! generate {
            () => { #[cfg(feature = "enabled")] pub fn generated() {} };
        }
    "#;
    for enabled in [true, false] {
        let (mut db, items, owner) = imported(definition);
        let root = db.add(
            "consumer.rs",
            "dep::generate!(); fn main() { generated(); }",
        );
        let values = if enabled {
            vec![(Symbol::intern("feature"), Symbol::intern("enabled"))]
        } else {
            vec![]
        };
        db.set_crate_cfg(
            root,
            flux_hir::CrateCfg {
                names: vec![],
                values,
            },
        )
        .unwrap();
        let hir = flux_hir::lower_source_on(
            &mut db,
            items,
            root,
            Symbol::intern("consumer"),
            vec![(Symbol::intern("dep"), owner)],
        );
        assert_eq!(
            hir.diagnostics.is_empty(),
            enabled,
            "enabled={enabled}: {:?}",
            hir.diagnostics
        );
    }
}

fn pointer_chain(count: usize) -> flux_hir::TypeRef {
    let mut ty = flux_hir::TypeRef::Never;
    for _ in 0..count {
        ty = flux_hir::TypeRef::Ptr {
            mutable: false,
            pointee: Box::new(ty),
        };
    }
    ty
}

fn encode_in_context<T: flux_hir::flib::Encode>(
    depth: u32,
    value: &T,
    e: &mut Encoder,
) -> Result<(), flux_hir::flib::Error> {
    if depth == 0 {
        value.encode(e)
    } else {
        e.nested(|e| encode_in_context(depth - 1, value, e))
    }
}
fn decode_in_context<T: flux_hir::flib::Decode>(
    depth: u32,
    d: &mut Decoder<'_>,
) -> Result<T, flux_hir::flib::Error> {
    if depth == 0 {
        T::decode(d)
    } else {
        d.nested(|d| decode_in_context(depth - 1, d))
    }
}

#[test]
fn recursive_item_encoders_match_the_decoder_depth_boundary() {
    use flux_hir::flib::{Decode, Encode, MAX_NESTING_DEPTH};
    // Every pointer consumes an enum record and a sized Box record. The leaf
    // consumes one more enum record, and the outer Box reaches exactly 128.
    let pointers = (MAX_NESTING_DEPTH / 2 - 1) as usize;
    let value = Box::new(pointer_chain(pointers));
    let mut encoder = Encoder::new();
    value.encode(&mut encoder).unwrap();
    let bytes = encoder.into_bytes();
    let mut decoder = Decoder::new(&bytes);
    assert_eq!(
        Box::<flux_hir::TypeRef>::decode(&mut decoder).unwrap(),
        value
    );
    decoder.finish().unwrap();
    let above = pointer_chain(pointers + 1);
    let mut encoder = Encoder::new();
    assert!(above.encode(&mut encoder).is_err());
    // An error must restore the depth budget while its partially written bytes
    // remain unusable. A subsequent independent record can still be encoded.
    value.encode(&mut encoder).unwrap();
}

#[test]
fn boxed_strings_and_slices_use_exact_matching_depth_counts() {
    use flux_hir::flib::MAX_NESTING_DEPTH;
    let text = "abc".to_owned().into_boxed_str();
    let mut encoder = Encoder::new();
    encode_in_context(MAX_NESTING_DEPTH, &text, &mut encoder).unwrap();
    let bytes = encoder.into_bytes();
    let mut decoder = Decoder::new(&bytes);
    assert_eq!(
        decode_in_context::<Box<str>>(MAX_NESTING_DEPTH, &mut decoder).unwrap(),
        text
    );
    decoder.finish().unwrap();
    let slice = vec![1u8, 2, 3].into_boxed_slice();
    let mut encoder = Encoder::new();
    encode_in_context(MAX_NESTING_DEPTH - 1, &slice, &mut encoder).unwrap();
    let bytes = encoder.into_bytes();
    let mut decoder = Decoder::new(&bytes);
    assert_eq!(
        decode_in_context::<Box<[u8]>>(MAX_NESTING_DEPTH - 1, &mut decoder).unwrap(),
        slice
    );
    decoder.finish().unwrap();
    assert!(encode_in_context(MAX_NESTING_DEPTH, &slice, &mut Encoder::new()).is_err());
}

#[test]
fn sorted_map_keys_preserve_the_ambient_nesting_budget() {
    use flux_hir::flib::MAX_NESTING_DEPTH;
    let mut map = std::collections::HashMap::new();
    map.insert(pointer_chain((MAX_NESTING_DEPTH / 2 - 1) as usize), 7u8);
    let mut encoder = Encoder::new();
    encode_in_context(1, &map, &mut encoder).unwrap();
    let bytes = encoder.into_bytes();
    let mut decoder = Decoder::new(&bytes);
    assert_eq!(
        decode_in_context::<std::collections::HashMap<flux_hir::TypeRef, u8>>(1, &mut decoder)
            .unwrap(),
        map
    );
    decoder.finish().unwrap();
    assert!(encode_in_context(2, &map, &mut Encoder::new()).is_err());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct CachedRecord(u32);
impl flux_hir::flib::Encode for CachedRecord {
    fn encode(&self, e: &mut Encoder) -> Result<(), flux_hir::flib::Error> {
        e.nested(|e| {
            if let Some((wire, _)) = e.type_reference(self.0)? {
                1u8.encode(e)?;
                wire.encode(e)
            } else {
                let wire = e.reserve_type(self.0)?;
                0u8.encode(e)?;
                wire.encode(e)?;
                self.0.encode(e)?;
                e.complete_type(self.0, 1)
            }
        })
    }
}
impl flux_hir::flib::Decode for CachedRecord {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, flux_hir::flib::Error> {
        d.nested(|d| {
            let tag = u8::decode(d)?;
            let wire = u32::decode(d)?;
            match tag {
                0 => {
                    d.reserve_type(wire)?;
                    let value = u32::decode(d)?;
                    d.complete_type(wire, value, 1)?;
                    Ok(Self(value))
                }
                1 => Ok(Self(d.type_reference(wire)?.0)),
                _ => Err(flux_hir::flib::Error("invalid test record tag")),
            }
        })
    }
}

#[test]
fn sorted_map_keys_share_the_main_stream_type_namespace() {
    use flux_hir::flib::{Decode, Encode};
    let mut map = std::collections::HashMap::new();
    map.insert(CachedRecord(7), CachedRecord(7));
    map.insert(CachedRecord(9), CachedRecord(9));
    let mut encoder = Encoder::new();
    map.encode(&mut encoder).unwrap();
    let bytes = encoder.into_bytes();
    let mut decoder = Decoder::new(&bytes);
    assert_eq!(
        std::collections::HashMap::<CachedRecord, CachedRecord>::decode(&mut decoder).unwrap(),
        map
    );
    decoder.finish().unwrap();
}

#[test]
fn type_cache_slots_reject_pending_forward_and_duplicate_references() {
    let mut encoder = Encoder::new();
    encoder.set_type_depth(123, 3).unwrap();
    assert_eq!(encoder.type_depth(123), Some(3));
    assert_eq!(encoder.type_reference(123).unwrap(), None);
    assert_eq!(encoder.reserve_type(123).unwrap(), 0);
    assert!(encoder.type_reference(123).is_err());
    encoder.complete_type(123, 3).unwrap();
    assert_eq!(encoder.type_reference(123).unwrap(), Some((0, 3)));
    assert!(encoder.reserve_type(123).is_err());
    assert!(encoder.complete_type(123, 3).is_err());
    assert!(encoder.set_type_depth(123, 2).is_err());
    let mut decoder = Decoder::new(&[]);
    assert!(decoder.type_reference(0).is_err());
    assert!(decoder.reserve_type(1).is_err());
    decoder.reserve_type(0).unwrap();
    assert!(decoder.type_reference(0).is_err());
    decoder.reserve_type(1).unwrap();
    decoder.complete_type(1, 777, 1).unwrap();
    assert_eq!(decoder.type_reference(1).unwrap(), (777, 1));
    assert_eq!(decoder.type_depth(777), Some(1));
    assert!(decoder.type_reference(0).is_err());
    decoder.complete_type(0, 888, 2).unwrap();
    assert_eq!(decoder.type_reference(0).unwrap(), (888, 2));
    decoder.reserve_type(2).unwrap();
    assert!(decoder.complete_type(2, 777, 1).is_err());
    assert!(decoder.complete_type(0, 888, 2).is_err());
    assert!(decoder.type_reference(3).is_err());
}

#[test]
fn generated_macro_preserves_keyword_edition_when_matcher_tokens_have_another_edition() {
    let mut original = SourceDb::new();
    let a=original.add_with_edition("old.rs",r#"#[macro_export] macro_rules! make { ($($tokens:tt)*) => { #[macro_export] macro_rules! generated { $($tokens)* } }; }"#,flux_syntax::Edition::E2021);
    let b = original.add_with_edition(
        "new.rs",
        r#"old::make! { ($value:expr) => { $value }; }"#,
        flux_syntax::Edition::E2024,
    );
    let source = flux_hir::lower_sources(
        &mut original,
        &[
            flux_hir::SourceCrate {
                root: a,
                name: Symbol::intern("old"),
                dependencies: Vec::new(),
            },
            flux_hir::SourceCrate {
                root: b,
                name: Symbol::intern("newer"),
                dependencies: vec![(Symbol::intern("old"), 0)],
            },
        ],
    );
    assert!(source.diagnostics.is_empty(), "{:?}", source.diagnostics);
    let owner = source.items.main_crate();
    let old = CrateId(owner.0 - 1);
    let mut db = SourceDb::new();
    let mut items = flux_hir::Core::get().items.clone();
    for krate in [old, owner] {
        let bytes = encode(&source.items, krate);
        let mut ids = IdMap::strict();
        for index in 0..krate.0 {
            let dep = CrateId(index);
            CrateIndices::collect(&items, dep)
                .add_decode_mapping(&mut ids, dep, index + 1)
                .unwrap();
        }
        let mut d = Decoder::with_ids(&bytes, ids);
        read_items(&mut items, &mut db, &mut d).unwrap();
        d.finish().unwrap();
    }
    let text = "fn main(){let _ = renamed::generated!(const { 1 });}";
    let root = original.add("consumer.rs", text);
    let expected = flux_hir::lower_source_on(
        &mut original,
        source.items,
        root,
        Symbol::intern("consumer"),
        vec![(Symbol::intern("renamed"), owner)],
    );
    assert!(
        !expected.diagnostics.is_empty(),
        "edition-2021 macro must reject const block expression"
    );
    let root = db.add("consumer.rs", text);
    let found = flux_hir::lower_source_on(
        &mut db,
        items,
        root,
        Symbol::intern("consumer"),
        vec![(Symbol::intern("renamed"), owner)],
    );
    assert_eq!(
        expected
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>(),
        found
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn allocation_slots_preserve_arc_identity_and_reject_invalid_references() {
    use std::sync::Arc;
    let owner = Arc::new(vec![3u8, 7]);
    let raw = Arc::as_ptr(&owner) as usize;
    let mut encoder = Encoder::new();
    encoder
        .set_allocation_depth(0, raw, owner.clone(), 2)
        .unwrap();
    assert_eq!(
        Arc::strong_count(&owner),
        2,
        "depth memo keeps an allocation alive"
    );
    assert_eq!(encoder.allocation_reference(0, raw).unwrap(), None);
    assert_eq!(
        encoder.reserve_allocation(0, raw, owner.clone()).unwrap(),
        0
    );
    assert!(encoder.allocation_reference(0, raw).is_err());
    encoder.complete_allocation(0, raw, 2).unwrap();
    assert_eq!(encoder.allocation_reference(0, raw).unwrap(), Some((0, 2)));
    assert!(encoder.reserve_allocation(0, raw, owner.clone()).is_err());
    assert!(
        encoder
            .set_allocation_depth(0, raw, owner.clone(), 3)
            .is_err()
    );
    assert!(encoder.retain_allocation(0, raw, Arc::new(5u32)).is_err());
    let weak = Arc::downgrade(&owner);
    drop(owner);
    assert!(
        weak.upgrade().is_some(),
        "encoder retains the actual Arc owner"
    );
    drop(encoder);
    assert!(weak.upgrade().is_none());

    let owner = Arc::<[u8]>::from(vec![4, 8]);
    let raw = Arc::as_ptr(&owner) as *const () as usize;
    let mut decoder = Decoder::new(&[]);
    assert!(decoder.allocation_reference::<Arc<[u8]>>(0, 0).is_err());
    assert!(decoder.reserve_allocation(0, 1).is_err());
    decoder.reserve_allocation(0, 0).unwrap();
    assert!(decoder.allocation_reference::<Arc<[u8]>>(0, 0).is_err());
    assert!(decoder.complete_allocation(1, 0, owner.clone(), 1).is_err());
    decoder
        .set_allocation_depth(0, raw, owner.clone(), 1)
        .unwrap();
    decoder.complete_allocation(0, 0, owner.clone(), 1).unwrap();
    let (alias, depth) = decoder.allocation_reference::<Arc<[u8]>>(0, 0).unwrap();
    assert!(Arc::ptr_eq(&owner, &alias));
    assert_eq!(depth, 1);
    assert_eq!(decoder.allocation_depth(0, raw), Some(1));
    assert!(decoder.allocation_reference::<Arc<[u8]>>(1, 0).is_err());
    assert!(decoder.allocation_reference::<Arc<Vec<u8>>>(0, 0).is_err());
    assert!(decoder.complete_allocation(0, 0, owner, 1).is_err());
}

#[test]
fn allocation_domains_and_type_slots_have_independent_namespaces() {
    use std::sync::Arc;
    let owner = Arc::new(11u32);
    let raw = Arc::as_ptr(&owner) as usize;
    let mut encoder = Encoder::new();
    assert_eq!(encoder.reserve_type(5).unwrap(), 0);
    encoder.complete_type(5, 1).unwrap();
    encoder
        .set_allocation_depth(1, raw, owner.clone(), 1)
        .unwrap();
    assert_eq!(
        encoder.reserve_allocation(0, raw, owner.clone()).unwrap(),
        0
    );
    encoder.complete_allocation(0, raw, 1).unwrap();
    assert_eq!(
        encoder.reserve_allocation(1, raw, owner.clone()).unwrap(),
        1
    );
    encoder.complete_allocation(1, raw, 1).unwrap();
    assert_eq!(encoder.type_reference(5).unwrap(), Some((0, 1)));
    assert_eq!(encoder.allocation_reference(0, raw).unwrap(), Some((0, 1)));
    assert_eq!(encoder.allocation_reference(1, raw).unwrap(), Some((1, 1)));
    assert!(
        encoder
            .set_allocation_depth(2, raw, owner.clone(), 0)
            .is_err()
    );
    assert!(encoder.set_allocation_depth(2, raw, owner, 129).is_err());
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CachedAllocation(std::sync::Arc<u32>);
impl flux_hir::flib::Encode for CachedAllocation {
    fn encode(&self, e: &mut Encoder) -> Result<(), flux_hir::flib::Error> {
        e.nested(|e| {
            let raw = std::sync::Arc::as_ptr(&self.0) as usize;
            if let Some((wire, _)) = e.allocation_reference(42, raw)? {
                1u8.encode(e)?;
                wire.encode(e)
            } else {
                let wire = e.reserve_allocation(42, raw, self.0.clone())?;
                0u8.encode(e)?;
                wire.encode(e)?;
                (*self.0).encode(e)?;
                e.complete_allocation(42, raw, 1)
            }
        })
    }
}
impl flux_hir::flib::Decode for CachedAllocation {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, flux_hir::flib::Error> {
        d.nested(|d| {
            let tag = u8::decode(d)?;
            let wire = u32::decode(d)?;
            match tag {
                0 => {
                    d.reserve_allocation(42, wire)?;
                    let owner = std::sync::Arc::new(u32::decode(d)?);
                    d.complete_allocation(42, wire, owner.clone(), 1)?;
                    Ok(Self(owner))
                }
                1 => Ok(Self(
                    d.allocation_reference::<std::sync::Arc<u32>>(42, wire)?.0,
                )),
                _ => Err(flux_hir::flib::Error("invalid test allocation tag")),
            }
        })
    }
}

#[test]
fn sorted_map_allocation_keys_keep_aliases_in_the_main_namespace() {
    use flux_hir::flib::{Decode, Encode};
    use std::sync::Arc;
    let first = CachedAllocation(Arc::new(7));
    let second = CachedAllocation(Arc::new(9));
    let mut map = std::collections::HashMap::new();
    map.insert(first.clone(), first);
    map.insert(second.clone(), second);
    let mut encoder = Encoder::new();
    map.encode(&mut encoder).unwrap();
    let bytes = encoder.into_bytes();
    let mut decoder = Decoder::new(&bytes);
    let decoded =
        std::collections::HashMap::<CachedAllocation, CachedAllocation>::decode(&mut decoder)
            .unwrap();
    decoder.finish().unwrap();
    assert_eq!(decoded, map);
    for (key, value) in decoded {
        assert!(Arc::ptr_eq(&key.0, &value.0));
    }
}
