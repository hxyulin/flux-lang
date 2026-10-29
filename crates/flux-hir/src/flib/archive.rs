//! Export and append one crate's semantic item records.
use super::{CrateIndices, Decode, Decoder, Encode, Encoder, Error, IdKind};
use crate::{CrateId, FileId, ItemTree, SourceDb};
use flux_arena::Idx;
use std::sync::Arc;

/// Writes one crate's names, items, scopes, macro tokens and diagnostic sources.
/// The encoder must map this crate at slot zero and its dependency references.
pub fn write_items(items: &ItemTree, krate: CrateId, e: &mut Encoder) -> Result<(), Error> {
    let indices = CrateIndices::collect(items, krate);
    for (_, table) in indices.tables() {
        table.len().encode(e)?;
    }
    for &index in &indices.files {
        let file = items
            .files
            .get(&FileId(index))
            .ok_or(Error("missing artifact diagnostic source"))?;
        file.path.encode(e)?;
        file.text().encode(e)?;
        file.edition.encode(e)?;
        (file.range_base != 0).encode(e)?;
        file.expansion_of
            .map(|x| e.ids.get(IdKind::File, x.0))
            .transpose()?
            .encode(e)?;
    }
    items
        .crates
        .get(krate.0 as usize)
        .ok_or(Error("invalid crate reference"))?
        .encode(e)?;
    macro_rules! table {
        ($field:ident) => {
            for &index in &indices.$field {
                items.$field[Idx::from_raw(index)].encode(e)?;
            }
        };
    }
    table!(modules);
    table!(functions);
    table!(structs);
    table!(traits);
    table!(impls);
    table!(consts);
    table!(type_aliases);
    table!(macros);
    table!(macro_scopes);
    for &index in &indices.modules {
        items
            .def_map
            .scopes
            .get(index as usize)
            .ok_or(Error("missing module scope"))?
            .encode(e)?;
    }
    let uses: Vec<_> = items
        .uses
        .iter()
        .filter(|x| e.ids.contains_owned(IdKind::Module, x.module.into_raw()))
        .collect();
    uses.encode(e)?;
    let sized: rustc_hash::FxHashMap<_, _> = items
        .sized_dispatch
        .iter()
        .filter(|(id, _)| e.ids.contains_owned(IdKind::Function, id.into_raw()))
        .map(|(&k, v)| (k, v))
        .collect();
    sized.encode(e)?;
    let assoc: rustc_hash::FxHashMap<_, _> = items
        .assoc
        .iter()
        .filter_map(|(&key, values)| {
            let values: Vec<_> = values
                .iter()
                .copied()
                .filter(|v| match v {
                    crate::AssocItem::Function(id) => {
                        e.ids.contains_owned(IdKind::Function, id.into_raw())
                    }
                    crate::AssocItem::Const(id) => {
                        e.ids.contains_owned(IdKind::Const, id.into_raw())
                    }
                })
                .collect();
            (!values.is_empty()).then_some((key, values))
        })
        .collect();
    assoc.encode(e)?;
    let impls: rustc_hash::FxHashMap<_, _> = items
        .trait_impls
        .iter()
        .filter_map(|(&key, values)| {
            let values: Vec<_> = values
                .iter()
                .copied()
                .filter(|id| e.ids.contains_owned(IdKind::Impl, id.into_raw()))
                .collect();
            (!values.is_empty()).then_some((key, values))
        })
        .collect();
    impls.encode(e)?;
    let ctors: rustc_hash::FxHashMap<_, _> = items
        .ctor_fns
        .iter()
        .filter(|(_, id)| e.ids.contains_owned(IdKind::Function, id.into_raw()))
        .map(|(&k, &v)| (k, v))
        .collect();
    ctors.encode(e)?;
    let defaults: rustc_hash::FxHashMap<_, _> = items
        .default_variants
        .iter()
        .filter(|(id, _)| e.ids.contains_owned(IdKind::Struct, id.into_raw()))
        .map(|(&k, &v)| (k, v))
        .collect();
    defaults.encode(e)?;
    let exported: rustc_hash::FxHashMap<_, _> = items
        .exported_macros
        .iter()
        .filter(|((owner, _), _)| *owner == krate)
        .map(|(&k, &v)| (k, v))
        .collect();
    exported.encode(e)?;
    let blocks: rustc_hash::FxHashMap<_, _> = items
        .block_modules
        .iter()
        .filter(|(_, id)| e.ids.contains_owned(IdKind::Module, id.into_raw()))
        .map(|(&k, &v)| (k, v))
        .collect();
    blocks.encode(e)?;
    let constants: rustc_hash::FxHashMap<_, _> = items
        .anon_consts
        .iter()
        .filter(|(_, id)| e.ids.contains_owned(IdKind::Const, id.into_raw()))
        .map(|(&k, &v)| (k, v))
        .collect();
    constants.encode(e)?;
    items.native_libraries.encode(e)
}

/// Appends decoded records. Dependency mappings must already be installed.
/// Returns the loaded crate and its process ownership map for downstream imports.
pub fn read_items(
    items: &mut ItemTree,
    db: &mut SourceDb,
    d: &mut Decoder<'_>,
) -> Result<(CrateId, CrateIndices), Error> {
    if !d.ids.is_strict() {
        return Err(Error("item loading requires checked reference mappings"));
    }
    let mut counts = Vec::new();
    for _ in 0..10 {
        counts.push(d.length()?);
    }
    let krate = CrateId(u32::try_from(items.crates.len()).map_err(|_| Error("too many crates"))?);
    let mut indices = CrateIndices::default();
    macro_rules! reserve {
        ($field:ident,$n:expr) => {{
            let start = items.$field.len();
            let end = start
                .checked_add(counts[$n])
                .ok_or(Error("item index overflow"))?;
            if end > u32::MAX as usize {
                return Err(Error("item index overflow"));
            }
            indices.$field = (start..end).map(|x| x as u32).collect();
        }};
    }
    reserve!(modules, 0);
    reserve!(functions, 1);
    reserve!(structs, 2);
    reserve!(traits, 3);
    reserve!(impls, 4);
    reserve!(consts, 5);
    reserve!(type_aliases, 6);
    reserve!(macros, 7);
    reserve!(macro_scopes, 8);
    struct Source {
        path: std::path::PathBuf,
        text: String,
        edition: flux_syntax::Edition,
        included: bool,
        expansion: Option<u32>,
    }
    let mut sources = Vec::new();
    for _ in 0..counts[9] {
        sources.push(Source {
            path: Decode::decode(d)?,
            text: Decode::decode(d)?,
            edition: Decode::decode(d)?,
            included: Decode::decode(d)?,
            expansion: Decode::decode(d)?,
        });
    }
    // File slots are reserved by appending diagnostic-only records. They cannot
    // become semantic inputs if a later record rejects the artifact.
    for source in &sources {
        let file = db.add_metadata_source(source.path.clone(), source.text.clone(), source.edition);
        if source.included {
            let base = db.allocate_metadata_include(file)?;
            indices
                .source_ranges
                .push((file.0, base, base + source.text.len() as u32));
        }
        indices.files.push(file.0);
    }
    indices.add_decode_mapping(&mut d.ids, krate, 0)?;
    let data = crate::item_tree::CrateData::decode(d)?;
    let modules: Vec<crate::Module> = read_records(counts[0], d)?;
    let functions: Vec<crate::Function> = read_records(counts[1], d)?;
    let structs: Vec<crate::Struct> = read_records(counts[2], d)?;
    let traits: Vec<crate::Trait> = read_records(counts[3], d)?;
    let impls: Vec<crate::Impl> = read_records(counts[4], d)?;
    let consts: Vec<crate::Const> = read_records(counts[5], d)?;
    let aliases: Vec<crate::TypeAlias> = read_records(counts[6], d)?;
    let macros: Vec<crate::MacroDef> = read_records(counts[7], d)?;
    let scopes = read_records(counts[8], d)?;
    let def_scopes = read_records(counts[0], d)?;
    let uses: Vec<crate::item_tree::Use> = Decode::decode(d)?;
    let sized: rustc_hash::FxHashMap<crate::FunctionId, crate::SizedDispatch> = Decode::decode(d)?;
    let associated: rustc_hash::FxHashMap<
        (crate::ImplTarget, flux_intern::Symbol),
        Vec<crate::AssocItem>,
    > = Decode::decode(d)?;
    let trait_impls: rustc_hash::FxHashMap<crate::TraitId, Vec<crate::ImplId>> = Decode::decode(d)?;
    let constructors: rustc_hash::FxHashMap<(crate::StructId, u32), crate::FunctionId> =
        Decode::decode(d)?;
    let defaults: rustc_hash::FxHashMap<crate::StructId, u32> = Decode::decode(d)?;
    let exported: rustc_hash::FxHashMap<(CrateId, flux_intern::Symbol), crate::MacroId> =
        Decode::decode(d)?;
    let blocks: rustc_hash::FxHashMap<(FileId, flux_syntax::NodeId), crate::ModuleId> =
        Decode::decode(d)?;
    let anonymous: rustc_hash::FxHashMap<(FileId, flux_syntax::NodeId), crate::ConstId> =
        Decode::decode(d)?;
    let native: Vec<flux_intern::Symbol> = Decode::decode(d)?;
    let own_module = |id: crate::ModuleId| d.ids.contains_owned(IdKind::Module, id.into_raw());
    if !own_module(data.root) {
        return Err(Error("crate root is not owned by artifact"));
    }
    let root_index = indices
        .modules
        .iter()
        .position(|&id| id == data.root.into_raw())
        .ok_or(Error("missing artifact root module"))?;
    let root_module = &modules[root_index];
    if root_index != 0
        || root_module.parent.is_some()
        || root_module.name != data.name
        || !d.ids.contains_owned(IdKind::File, root_module.file.0)
    {
        return Err(Error("invalid artifact root ownership"));
    }
    for (&index, module) in indices.modules.iter().zip(&modules) {
        if module.krate != krate
            || module
                .parent
                .is_some_and(|p| !own_module(p) || p.into_raw() >= index)
        {
            return Err(Error("invalid artifact module ownership"));
        }
    }
    if functions.iter().any(|x| !own_module(x.loc.module))
        || structs.iter().any(|x| !own_module(x.loc.module))
        || traits.iter().any(|x| !own_module(x.loc.module))
        || impls.iter().any(|x| !own_module(x.module))
        || consts.iter().any(|x| !own_module(x.loc.module))
        || aliases.iter().any(|x| !own_module(x.loc.module))
        || macros.iter().any(|x| x.krate != krate)
        || uses.iter().any(|x| !own_module(x.module))
    {
        return Err(Error("item is not owned by artifact"));
    }
    if functions.iter().any(|x| {
        x.intrinsic.is_some()
            || x.inherits_overflow_checks
            || x.do_not_const_check
            || x.const_unstable
            || x.precise_live_drops
    }) || traits.iter().any(|x| x.lang.is_some())
        || consts.iter().any(|x| x.specialization)
    {
        return Err(Error("untrusted artifact contains privileged declarations"));
    }
    let own_function =
        |id: crate::FunctionId| d.ids.contains_owned(IdKind::Function, id.into_raw());
    let own_const = |id: crate::ConstId| d.ids.contains_owned(IdKind::Const, id.into_raw());
    let own_struct = |id: crate::StructId| d.ids.contains_owned(IdKind::Struct, id.into_raw());
    if !sized.is_empty()
        || constructors
            .iter()
            .any(|(&(id, _), &function)| !own_struct(id) || !own_function(function))
        || defaults.keys().any(|&id| !own_struct(id))
        || exported.iter().any(|(&(owner, _), &id)| {
            owner != krate || !d.ids.contains_owned(IdKind::Macro, id.into_raw())
        })
        || blocks
            .iter()
            .any(|(&(file, _), &id)| !d.ids.contains_owned(IdKind::File, file.0) || !own_module(id))
        || anonymous
            .iter()
            .any(|(&(file, _), &id)| !d.ids.contains_owned(IdKind::File, file.0) || !own_const(id))
        || associated.iter().any(|((target, _), values)| {
            matches!(target, crate::ImplTarget::Primitive(_))
                || values.iter().any(|value| match value {
                    crate::AssocItem::Function(id) => !own_function(*id),
                    crate::AssocItem::Const(id) => !own_const(*id),
                })
        })
        || trait_impls
            .values()
            .flatten()
            .any(|id| !d.ids.contains_owned(IdKind::Impl, id.into_raw()))
    {
        return Err(Error("artifact index contains nonowned declarations"));
    }
    let mut staged = items.clone();
    staged.crates.push(data);
    macro_rules! append {
        ($field:ident,$values:ident) => {
            for value in $values {
                staged.$field.alloc(value);
            }
        };
    }
    append!(modules, modules);
    append!(functions, functions);
    append!(structs, structs);
    append!(traits, traits);
    append!(impls, impls);
    append!(consts, consts);
    append!(type_aliases, aliases);
    append!(macros, macros);
    append!(macro_scopes, scopes);
    staged.def_map.scopes.extend(def_scopes);
    staged.uses.extend(uses);
    staged.sized_dispatch.extend(sized);
    for (key, values) in associated {
        staged.assoc.entry(key).or_default().extend(values);
    }
    for (key, values) in trait_impls {
        staged.trait_impls.entry(key).or_default().extend(values);
    }
    staged.ctor_fns.extend(constructors);
    staged.default_variants.extend(defaults);
    staged.exported_macros.extend(exported);
    staged.block_modules.extend(blocks);
    staged.anon_consts.extend(anonymous);
    staged.native_libraries.extend(native);
    for &index in &indices.macro_scopes {
        let scope = &staged.macro_scopes[Idx::from_raw(index)];
        if scope.parent.is_some_and(|p| p.into_raw() >= index)
            || scope.jump.is_some_and(|p| p.into_raw() >= index)
        {
            return Err(Error("invalid macro scope ancestry"));
        }
    }
    for &index in &indices.traits {
        let id = Idx::from_raw(index);
        for &function in &staged.traits[id].functions {
            staged
                .trait_fns
                .entry(staged.functions[function].name)
                .or_default()
                .push(id);
        }
        for &constant in &staged.traits[id].consts {
            if let Some(name) = staged.consts[constant].name {
                staged.trait_fns.entry(name).or_default().push(id);
            }
        }
    }
    for &index in &indices.macro_scopes {
        let id = Idx::from_raw(index);
        staged
            .macro_names
            .entry(staged.macro_scopes[id].name)
            .or_default()
            .push(id);
    }
    for (&index, source) in indices.files.iter().zip(sources) {
        let file = FileId(index);
        let expansion = source
            .expansion
            .map(|wire| d.ids.get(IdKind::File, wire).map(FileId))
            .transpose()?;
        let base = db.file(file).range_base;
        db.set_metadata_origin(file, base, expansion);
        let record = db.file(file).clone();
        if let Some(origin) = record.expansion_of {
            staged.expansion_files.insert(file, origin);
        }
        staged.files.insert(file, Arc::new(record));
    }
    let valid_file = |id: FileId| staged.files.contains_key(&id);
    for &index in &indices.modules {
        if !valid_file(staged.modules[Idx::from_raw(index)].file) {
            return Err(Error("invalid module diagnostic file"));
        }
    }
    for &index in &indices.functions {
        if !valid_file(staged.functions[Idx::from_raw(index)].loc.file) {
            return Err(Error("invalid function diagnostic file"));
        }
    }
    for &index in &indices.structs {
        if !valid_file(staged.structs[Idx::from_raw(index)].loc.file) {
            return Err(Error("invalid struct diagnostic file"));
        }
    }
    for &index in &indices.traits {
        if !valid_file(staged.traits[Idx::from_raw(index)].loc.file) {
            return Err(Error("invalid trait diagnostic file"));
        }
    }
    for &index in &indices.impls {
        if !valid_file(staged.impls[Idx::from_raw(index)].file) {
            return Err(Error("invalid impl diagnostic file"));
        }
    }
    for &index in &indices.consts {
        if !valid_file(staged.consts[Idx::from_raw(index)].loc.file) {
            return Err(Error("invalid constant diagnostic file"));
        }
    }
    for &index in &indices.type_aliases {
        if !valid_file(staged.type_aliases[Idx::from_raw(index)].loc.file) {
            return Err(Error("invalid alias diagnostic file"));
        }
    }
    for &index in &indices.macros {
        if !valid_file(staged.macros[Idx::from_raw(index)].file) {
            return Err(Error("invalid macro diagnostic file"));
        }
    }
    for &index in &indices.files {
        let mut file = FileId(index);
        let mut seen = std::collections::BTreeSet::new();
        while let Some(&parent) = staged.expansion_files.get(&file) {
            if !valid_file(parent) || !seen.insert(file) {
                return Err(Error("invalid expansion source ancestry"));
            }
            file = parent;
        }
    }
    staged
        .artifact_files
        .insert(krate, indices.files.iter().copied().map(FileId).collect());
    if indices.macros.iter().any(|&id| {
        matches!(
            staged.macros[Idx::from_raw(id)].kind,
            crate::MacroKind::ProcMacro(_)
        )
    }) {
        db.note_metadata_proc_macros();
    }
    *items = staged;
    Ok((krate, indices))
}
fn read_records<T: Decode>(count: usize, d: &mut Decoder<'_>) -> Result<Vec<T>, Error> {
    (0..count).map(|_| T::decode(d)).collect()
}
