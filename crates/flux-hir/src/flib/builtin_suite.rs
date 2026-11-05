//! Trusted compiler-library prefix. This entry point is separate from user crates.
//!
//! Resolved semantic tables survive loading. Source-only body expansion caches
//! and unresolved sized-dispatch declarations are omitted: this prefix must use
//! decoded checked MIR, and its HIR bodies cannot be lowered again.
use super::{Decode, Decoder, Encode, Encoder, Error, IdKind, IdMap};
use crate::{FileId, ItemTree, SourceDb};
use flux_arena::LayeredArena;

const MAGIC: &str = "FLUX-BUILTIN-HIR-1";
const KINDS: [IdKind; 11] = [
    IdKind::Crate,
    IdKind::Module,
    IdKind::Function,
    IdKind::Struct,
    IdKind::Trait,
    IdKind::Impl,
    IdKind::Const,
    IdKind::TypeAlias,
    IdKind::Macro,
    IdKind::MacroScope,
    IdKind::File,
];

fn identity(counts: &[usize; 11]) -> IdMap {
    let mut ids = IdMap::strict();
    for (&kind, &count) in KINDS.iter().zip(counts) {
        for raw in 0..count as u32 {
            ids.insert(kind, raw, raw);
            ids.mark_owned(kind, raw);
        }
    }
    ids.insert(IdKind::File, u32::MAX, u32::MAX);
    ids
}
fn write_arena<T: Encode>(arena: &LayeredArena<T>, e: &mut Encoder) -> Result<(), Error> {
    for (_, value) in arena.iter() {
        value.encode(e)?;
    }
    Ok(())
}
fn read_arena<T: Decode>(count: usize, d: &mut Decoder<'_>) -> Result<LayeredArena<T>, Error> {
    let mut arena = LayeredArena::new();
    for _ in 0..count {
        arena.alloc(T::decode(d)?);
    }
    Ok(arena)
}
fn roles(items: &ItemTree) -> Result<(), Error> {
    if items.crates.len() != 3
        || items
            .crates
            .iter()
            .map(|c| (c.name.as_str(), c.is_core))
            .collect::<Vec<_>>()
            != [("core", true), ("alloc", true), ("std", false)]
    {
        return Err(Error("expected exact core alloc std library suite"));
    }
    if items.def_map.scopes.len() != items.modules.len() {
        return Err(Error("builtin scope count mismatch"));
    }
    for (index, data) in items.crates.iter().enumerate() {
        let roots = [FileId::CORE_ROOT, FileId::alloc_root(), FileId::std_root()];
        if items
            .modules
            .get(data.root)
            .is_none_or(|module| module.krate.0 as usize != index || module.file != roots[index])
        {
            return Err(Error("invalid builtin crate root"));
        }
    }
    Ok(())
}

/// Writes the complete trusted compiler-library prefix and its exact source database.
/// The caller must bind this section to the current compiler build identity.
/// This format cannot be loaded through the ordinary user-crate loader.
pub fn write_builtin_suite(items: &ItemTree, db: &SourceDb, e: &mut Encoder) -> Result<(), Error> {
    roles(items)?;
    if !items.diagnostics.is_empty() {
        return Err(Error("cannot write erroneous builtin suite"));
    }
    for (&id, file) in &items.files {
        let Some((_, source)) = db.files().find(|(found, _)| *found == id) else {
            return Err(Error("builtin item source is absent from database"));
        };
        if file.path != source.path
            || file.text() != source.text()
            || file.edition != source.edition
            || file.range_base != source.range_base
            || file.expansion_of != source.expansion_of
        {
            return Err(Error("builtin item source disagrees with database"));
        }
    }
    let counts = [
        items.crates.len(),
        items.modules.len(),
        items.functions.len(),
        items.structs.len(),
        items.traits.len(),
        items.impls.len(),
        items.consts.len(),
        items.type_aliases.len(),
        items.macros.len(),
        items.macro_scopes.len(),
        db.files().count(),
    ];
    MAGIC.encode(e)?;
    for count in counts {
        if count > super::MAX_RECORD_LENGTH {
            return Err(Error("builtin table is too large"));
        }
        count.encode(e)?;
    }
    e.ids = identity(&counts);
    for (id, file) in db.files() {
        if file.range_base != 0 {
            e.ids.add_source_range(
                id.0,
                file.range_base,
                file.range_base + file.text().len() as u32,
            );
        }
    }
    for (_, file) in db.files() {
        file.path.encode(e)?;
        file.text().encode(e)?;
        file.edition.encode(e)?;
        file.range_base.encode(e)?;
        file.expansion_of.encode(e)?;
    }
    let previous = e.trusted_builtin_suite;
    e.trusted_builtin_suite = true;
    let result = write_records(items, e);
    e.trusted_builtin_suite = previous;
    result
}
fn write_records(items: &ItemTree, e: &mut Encoder) -> Result<(), Error> {
    items.crates.encode(e)?;
    macro_rules! arena {($($field:ident),*)=>{$(write_arena(&items.$field,e)?;)*};}
    arena!(
        modules,
        functions,
        structs,
        traits,
        impls,
        consts,
        type_aliases,
        macros,
        macro_scopes
    );
    macro_rules! fields {($($field:ident),*)=>{$(items.$field.encode(e)?;)*};}
    fields!(
        sized_dispatch,
        uses,
        native_libraries,
        assoc,
        trait_impls,
        trait_fns,
        block_modules,
        anon_consts,
        lang_traits,
        lang_enums,
        panic_fn,
        lang_fns,
        ctor_fns,
        default_variants,
        macro_names,
        exported_macros,
        macro_prelude,
        expansion_files
    );
    let mut files: Vec<_> = items.files.keys().copied().collect();
    files.sort_by_key(|id| id.0);
    files.encode(e)?;
    items.def_map.scopes.encode(e)?;
    items.def_map.prelude.encode(e)?;
    items.def_map.std_prelude.encode(e)?;
    items.def_map.prelude_2018.encode(e)?;
    items.def_map.std_prelude_2018.encode(e)?;
    if !items.def_map.failed_imports.is_empty() {
        return Err(Error("builtin suite has failed imports"));
    }
    Ok(())
}

/// Restores trusted compiler libraries into an empty database with exact prefix IDs.
/// Only call after the enclosing artifact's compiler identity has been verified.
pub fn read_builtin_suite(d: &mut Decoder<'_>) -> Result<(SourceDb, ItemTree), Error> {
    if String::decode(d)? != MAGIC {
        return Err(Error("invalid builtin HIR suite magic"));
    }
    let mut counts = [0; 11];
    for count in &mut counts {
        *count = d.length()?;
    }
    if counts[0] != 3 {
        return Err(Error("invalid builtin crate count"));
    }
    d.ids = identity(&counts);
    let mut db = SourceDb::empty();
    for raw in 0..counts[10] {
        let path = Decode::decode(d)?;
        let text: String = Decode::decode(d)?;
        let edition = Decode::decode(d)?;
        let base = u32::decode(d)?;
        let origin = Option::<FileId>::decode(d)?;
        if origin.is_some_and(|id| id.0 as usize >= counts[10]) {
            return Err(Error("invalid builtin source origin"));
        }
        let end = base
            .checked_add(
                u32::try_from(text.len()).map_err(|_| Error("builtin source is too large"))?,
            )
            .ok_or(Error("builtin source coordinate overflow"))?;
        let id = db.add_metadata_source(path, text, edition);
        if id.0 as usize != raw {
            return Err(Error("builtin file order mismatch"));
        }
        db.set_metadata_origin(id, base, origin);
        if base != 0 {
            d.ids.add_source_range(id.0, base, end);
        }
    }
    let previous = d.trusted_builtin_suite;
    d.trusted_builtin_suite = true;
    let result = read_records(&counts, &db, d);
    d.trusted_builtin_suite = previous;
    let mut items = result?;
    roles(&items)?;
    items.core = items.counts();
    items.artifact_builtins = true;
    crate::library::note_library(&items);
    macro_rules! freeze {($($field:ident),*)=>{$(items.$field.freeze();)*};}
    freeze!(
        modules,
        functions,
        structs,
        traits,
        impls,
        consts,
        type_aliases,
        macros,
        macro_scopes
    );
    Ok((SourceDb::with_files(db.all_files()), items))
}
fn read_records(
    counts: &[usize; 11],
    db: &SourceDb,
    d: &mut Decoder<'_>,
) -> Result<ItemTree, Error> {
    let mut items = ItemTree::default();
    items.crates = Decode::decode(d)?;
    if items.crates.len() != counts[0] {
        return Err(Error("builtin crate count mismatch"));
    }
    items.modules = read_arena(counts[1], d)?;
    items.functions = read_arena(counts[2], d)?;
    items.structs = read_arena(counts[3], d)?;
    items.traits = read_arena(counts[4], d)?;
    items.impls = read_arena(counts[5], d)?;
    items.consts = read_arena(counts[6], d)?;
    items.type_aliases = read_arena(counts[7], d)?;
    items.macros = read_arena(counts[8], d)?;
    items.macro_scopes = read_arena(counts[9], d)?;
    macro_rules! fields {($($field:ident),*)=>{$(items.$field=Decode::decode(d)?;)*};}
    fields!(
        sized_dispatch,
        uses,
        native_libraries,
        assoc,
        trait_impls,
        trait_fns,
        block_modules,
        anon_consts,
        lang_traits,
        lang_enums,
        panic_fn,
        lang_fns,
        ctor_fns,
        default_variants,
        macro_names,
        exported_macros,
        macro_prelude,
        expansion_files
    );
    let files = Vec::<FileId>::decode(d)?;
    for id in files {
        if id.0 as usize >= db.files().count() {
            return Err(Error("invalid builtin source file"));
        }
        if items.insert_file(id, db.shared(id)).is_some() {
            return Err(Error("duplicate builtin source"));
        }
    }
    items.def_map.scopes = Decode::decode(d)?;
    items.def_map.prelude = Decode::decode(d)?;
    items.def_map.std_prelude = Decode::decode(d)?;
    items.def_map.prelude_2018 = Decode::decode(d)?;
    items.def_map.std_prelude_2018 = Decode::decode(d)?;
    Ok(items)
}
