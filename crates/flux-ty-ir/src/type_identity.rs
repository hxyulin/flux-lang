//! Stable runtime type identities shared by evaluation and monomorphization.

use std::sync::{Arc, Mutex};

use flux_hir::flib::{Encode, Encoder, IdKind, IdMap};
use flux_hir::{CrateId, ItemTree};
use rustc_hash::FxHashMap;

use crate::{ClosureOwner, Ty, TyKind};

/// Exact identities supplied by an artifact compilation graph.
pub type CrateHashes = Vec<(CrateId, [u8; 32])>;

/// Shared identity cache and the exact crate hashes of a compilation graph.
#[derive(Debug, Clone, Default)]
pub struct TypeIds(
    pub Arc<Mutex<FxHashMap<Ty, u128>>>,
    pub Arc<Mutex<CrateHashes>>,
);

impl TypeIds {
    /// Creates an independent identity cache for another compilation graph.
    pub fn fork(&self) -> Self {
        Self(
            Arc::new(Mutex::new(FxHashMap::default())),
            Arc::new(Mutex::new(
                self.1.lock().expect("type identity lock poisoned").clone(),
            )),
        )
    }

    /// Installs crate identities before constant evaluation or code generation.
    pub fn set_crate_hashes(&self, hashes: &[(CrateId, [u8; 32])]) {
        *self.1.lock().expect("type identity lock poisoned") = hashes.to_vec();
        self.0.lock().expect("type identity lock poisoned").clear();
    }

    /// Assigns a deterministic ID to an already normalized type.
    /// A fingerprint collision is an internal error rather than an alias.
    pub fn of(&self, items: &ItemTree, canonical_ty: Ty) -> Result<u128, &'static str> {
        if let Some(&id) = self
            .0
            .lock()
            .expect("type identity lock poisoned")
            .get(&canonical_ty)
        {
            return Ok(id);
        }
        let hashes = self.1.lock().expect("type identity lock poisoned");
        let fingerprint = stable_id(items, &hashes, canonical_ty)?;
        let mut ids = self.0.lock().expect("type identity lock poisoned");
        if ids
            .iter()
            .any(|(&key, &id)| id == fingerprint && key != canonical_ty)
        {
            return Err("type identity fingerprint collision");
        }
        ids.insert(canonical_ty, fingerprint);
        Ok(fingerprint)
    }
}

/// Stable definition identity: owning crate, definition path and disambiguator.
/// The ordinal disambiguates repeated names, impl methods and anonymous items.
/// Explicit hashes carry package/source/configuration identity for artifacts.
pub fn definition_key(
    items: &ItemTree,
    hashes: &[(CrateId, [u8; 32])],
    kind: IdKind,
    raw: u32,
) -> Result<Vec<u8>, &'static str> {
    definition_key_inner(items, hashes, kind, raw, None)
}

/// Encodes a definition using a crate-local ordinal computed by the caller.
/// This avoids rescanning arenas when naming every definition in a crate.
pub fn definition_key_with_ordinal(
    items: &ItemTree,
    hashes: &[(CrateId, [u8; 32])],
    kind: IdKind,
    raw: u32,
    ordinal: u32,
) -> Result<Vec<u8>, &'static str> {
    definition_key_inner(items, hashes, kind, raw, Some(ordinal as usize))
}

fn definition_key_inner(
    items: &ItemTree,
    hashes: &[(CrateId, [u8; 32])],
    kind: IdKind,
    raw: u32,
    ordinal: Option<usize>,
) -> Result<Vec<u8>, &'static str> {
    let krate = definition_crate(items, kind, raw)?;
    let ordinal = match ordinal {
        Some(ordinal) => ordinal,
        None => {
            let crate_of = |loc: flux_hir::ItemLoc| items.modules[loc.module].krate;
            let before = |n: usize| match kind {
                IdKind::Struct => (items.structs.iter().take(n))
                    .filter(|(_, i)| crate_of(i.loc) == krate)
                    .count(),
                IdKind::Trait => (items.traits.iter().take(n))
                    .filter(|(_, i)| crate_of(i.loc) == krate)
                    .count(),
                IdKind::Function => (items.functions.iter().take(n))
                    .filter(|(_, i)| crate_of(i.loc) == krate)
                    .count(),
                IdKind::Const => (items.consts.iter().take(n))
                    .filter(|(_, i)| crate_of(i.loc) == krate)
                    .count(),
                _ => unreachable!("definition_crate accepts these kinds only"),
            };
            before(raw as usize)
        }
    };
    let prefix = crate_identity(items, hashes, krate)?;
    definition_key_after(items, &prefix, kind, raw, ordinal)
}

/// The crate that defines definition `raw` of `kind`.
fn definition_crate(items: &ItemTree, kind: IdKind, raw: u32) -> Result<CrateId, &'static str> {
    let loc = match kind {
        IdKind::Struct => {
            (items.structs.get(flux_hir::StructId::from_raw(raw)))
                .ok_or("invalid type definition")?
                .loc
        }
        IdKind::Trait => {
            (items.traits.get(flux_hir::TraitId::from_raw(raw)))
                .ok_or("invalid trait definition")?
                .loc
        }
        IdKind::Function => {
            (items.functions.get(flux_hir::FunctionId::from_raw(raw)))
                .ok_or("invalid function definition")?
                .loc
        }
        IdKind::Const => {
            (items.consts.get(flux_hir::ConstId::from_raw(raw)))
                .ok_or("invalid constant definition")?
                .loc
        }
        _ => return Err("unsupported type definition kind"),
    };
    Ok(items.modules[loc.module].krate)
}

/// The start of every definition key of `krate`: the key's version and
/// the crate's identity.
fn crate_identity(
    items: &ItemTree,
    hashes: &[(CrateId, [u8; 32])],
    krate: CrateId,
) -> Result<Vec<u8>, &'static str> {
    let mut e = Encoder::new();
    e.write(b"flux-definition-v2");
    if let Some((_, hash)) = hashes.iter().find(|(id, _)| *id == krate) {
        e.write(hash);
    } else {
        // Standalone graphs have no package hash yet. Paths disambiguate
        // same-named source crates; artifact drivers always provide hashes.
        let data = &items.crates[krate.0 as usize];
        data.name
            .encode(&mut e)
            .map_err(|_| "invalid crate identity")?;
        let root = &items.modules[data.root];
        let file = items
            .files
            .get(&root.file)
            .ok_or("missing crate root source identity")?;
        file.path.encode(&mut e).map_err(|_| "invalid crate path")?;
        let source_hash = flux_hir::flib::digest(file.parse.tree().text().as_bytes());
        e.write(&source_hash);
        format!("{:?}", data.edition)
            .encode(&mut e)
            .map_err(|_| "invalid crate edition")?;
    }
    Ok(e.into_bytes())
}

/// The key of definition `raw` of `kind`, whose crate's identity is
/// `prefix` and whose crate-local ordinal is `ordinal`.
fn definition_key_after(
    items: &ItemTree,
    prefix: &[u8],
    kind: IdKind,
    raw: u32,
    ordinal: usize,
) -> Result<Vec<u8>, &'static str> {
    let mut e = Encoder::new();
    e.write(prefix);
    let (module, name, file, range) = match kind {
        IdKind::Struct => {
            let i = &items.structs[flux_hir::StructId::from_raw(raw)];
            (i.loc.module, i.name.to_string(), i.loc.file, i.def_range)
        }
        IdKind::Trait => {
            let i = &items.traits[flux_hir::TraitId::from_raw(raw)];
            (i.loc.module, i.name.to_string(), i.loc.file, i.def_range)
        }
        IdKind::Function => {
            let i = &items.functions[flux_hir::FunctionId::from_raw(raw)];
            // The crate-local ordinal disambiguates impls. Diagnostic paths
            // can contain `$crate` anchors with process-local crate indices.
            (i.loc.module, i.name.to_string(), i.loc.file, i.def_range)
        }
        IdKind::Const => {
            let i = &items.consts[flux_hir::ConstId::from_raw(raw)];
            (
                i.loc.module,
                items.const_name(flux_hir::ConstId::from_raw(raw)),
                i.loc.file,
                i.def_range,
            )
        }
        _ => return Err("unsupported type definition kind"),
    };
    let mut modules = Vec::new();
    let mut current = Some(module);
    while let Some(id) = current {
        let module = &items.modules[id];
        let span = if module.block.is_some() {
            Some(span_key(items, module.file, module.def_range)?)
        } else {
            None
        };
        modules.push((module.name, span));
        current = module.parent;
    }
    modules.reverse();
    modules
        .encode(&mut e)
        .map_err(|_| "invalid definition path")?;
    name.encode(&mut e).map_err(|_| "invalid definition name")?;
    span_key(items, file, range)?
        .encode(&mut e)
        .map_err(|_| "invalid definition range")?;
    let tag = match kind {
        IdKind::Struct => 0u8,
        IdKind::Trait => 1,
        IdKind::Function => 2,
        IdKind::Const => 3,
        _ => unreachable!(),
    };
    tag.encode(&mut e).map_err(|_| "invalid type definition")?;
    u32::try_from(ordinal)
        .map_err(|_| "type definition index overflow")?
        .encode(&mut e)
        .map_err(|_| "invalid type definition")?;
    Ok(e.into_bytes())
}

/// The definition keys of one item tree under one set of crate hashes,
/// each computed once, with the crate-local ordinals and crate identities
/// they share: for naming many definitions and types of one compilation.
pub struct DefinitionKeys<'a> {
    items: &'a ItemTree,
    hashes: &'a [(CrateId, [u8; 32])],
    /// Each crate's identity, by crate index, once a key needs it.
    crates: Vec<Option<Box<[u8]>>>,
    /// Crate-local ordinals by raw index, per kind (structs, traits,
    /// functions, constants), once a key of that kind needs them.
    ordinals: [Option<Box<[u32]>>; 4],
    keys: FxHashMap<(u8, u32), Box<[u8]>>,
}

impl<'a> DefinitionKeys<'a> {
    pub fn new(items: &'a ItemTree, hashes: &'a [(CrateId, [u8; 32])]) -> Self {
        DefinitionKeys {
            items,
            hashes,
            crates: vec![None; items.crates.len()],
            ordinals: Default::default(),
            keys: FxHashMap::default(),
        }
    }

    /// What [`definition_key`] returns for the same arguments.
    pub fn key(&mut self, kind: IdKind, raw: u32) -> Result<&[u8], &'static str> {
        let slot = Self::slot(kind)?;
        if !self.keys.contains_key(&(slot, raw)) {
            let items = self.items;
            let krate = definition_crate(items, kind, raw)?;
            let ordinal = self.ordinals(kind)?[raw as usize];
            let prefix = match &mut self.crates[krate.0 as usize] {
                Some(prefix) => prefix,
                empty => empty.insert(crate_identity(items, self.hashes, krate)?.into()),
            };
            let key = definition_key_after(items, prefix, kind, raw, ordinal as usize)?;
            self.keys.insert((slot, raw), key.into());
        }
        Ok(&self.keys[&(slot, raw)])
    }

    /// What [`stable_key`] returns for the same type.
    pub fn stable_key(&mut self, canonical_ty: Ty) -> Result<Vec<u8>, &'static str> {
        stable_key_with(canonical_ty, |kind, raw| {
            self.key(kind, raw).map(<[u8]>::to_vec)
        })
    }

    /// The crate-local ordinal of every definition of `kind`, by raw index.
    pub fn ordinals(&mut self, kind: IdKind) -> Result<&[u32], &'static str> {
        let items = self.items;
        let slot = &mut self.ordinals[usize::from(Self::slot(kind)?)];
        Ok(slot.get_or_insert_with(|| {
            let modules: Vec<(u32, flux_hir::ModuleId)> = match kind {
                IdKind::Struct => (items.structs.iter())
                    .map(|(id, i)| (id.into_raw(), i.loc.module))
                    .collect(),
                IdKind::Trait => (items.traits.iter())
                    .map(|(id, i)| (id.into_raw(), i.loc.module))
                    .collect(),
                IdKind::Function => (items.functions.iter())
                    .map(|(id, i)| (id.into_raw(), i.loc.module))
                    .collect(),
                IdKind::Const => (items.consts.iter())
                    .map(|(id, i)| (id.into_raw(), i.loc.module))
                    .collect(),
                _ => unreachable!("slot accepts these kinds only"),
            };
            let mut counts = vec![0u32; items.crates.len()];
            let len = modules.iter().map(|&(raw, _)| raw as usize + 1).max();
            let mut ordinals = vec![0; len.unwrap_or(0)];
            for (raw, module) in modules {
                let count = &mut counts[items.modules[module].krate.0 as usize];
                ordinals[raw as usize] = *count;
                *count += 1;
            }
            ordinals.into()
        }))
    }

    fn slot(kind: IdKind) -> Result<u8, &'static str> {
        match kind {
            IdKind::Struct => Ok(0),
            IdKind::Trait => Ok(1),
            IdKind::Function => Ok(2),
            IdKind::Const => Ok(3),
            _ => Err("unsupported type definition kind"),
        }
    }
}

fn span_key(
    items: &ItemTree,
    file: flux_hir::FileId,
    range: flux_syntax::TextRange,
) -> Result<(std::path::PathBuf, flux_syntax::TextRange), &'static str> {
    let (file, range) = items.source_span(file, range);
    let source = items
        .files
        .get(&file)
        .ok_or("missing definition source identity")?;
    Ok((source.path.clone(), range))
}

/// Encodes a fully normalized type with process-independent references.
/// Normalization and function-pointer binder canonicalization remain solver work.
pub fn stable_key(
    items: &ItemTree,
    hashes: &[(CrateId, [u8; 32])],
    canonical_ty: Ty,
) -> Result<Vec<u8>, &'static str> {
    stable_key_with(canonical_ty, |kind, raw| {
        definition_key(items, hashes, kind, raw)
    })
}

fn stable_key_with(
    canonical_ty: Ty,
    mut definition_key: impl FnMut(IdKind, u32) -> Result<Vec<u8>, &'static str>,
) -> Result<Vec<u8>, &'static str> {
    let mut refs = std::collections::BTreeSet::new();
    for ty in identity_postorder(canonical_ty)? {
        match ty.kind() {
            TyKind::Adt(adt, _) => {
                refs.insert((IdKind::Struct, adt.strukt.into_raw()));
            }
            TyKind::Dynamic(d) => {
                refs.insert((IdKind::Trait, d.trait_.into_raw()));
                for auto in d.auto_traits {
                    refs.insert((IdKind::Trait, auto.trait_.into_raw()));
                }
            }
            TyKind::FnDef(f) => {
                refs.insert((IdKind::Function, f.function.into_raw()));
            }
            TyKind::Closure(id, _) => {
                refs.insert(match id.owner {
                    ClosureOwner::Function(f) => (IdKind::Function, f.into_raw()),
                    ClosureOwner::Const(c) => (IdKind::Const, c.into_raw()),
                });
            }
            TyKind::Alias(_)
            | TyKind::Opaque(_)
            | TyKind::Param(_)
            | TyKind::Infer(_)
            | TyKind::Error => return Err("type identity requires a fully resolved type"),
            _ => {}
        }
    }
    let mut definitions = Vec::new();
    for (kind, raw) in refs {
        definitions.push((definition_key(kind, raw)?, kind, raw));
    }
    definitions.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    let mut ids = IdMap::strict();
    let mut prefix = Encoder::new();
    prefix.write(b"flux-type-id-v2");
    definitions
        .len()
        .encode(&mut prefix)
        .map_err(|_| "type identity overflow")?;
    for (index, (key, kind, raw)) in definitions.into_iter().enumerate() {
        key.encode(&mut prefix)
            .map_err(|_| "type identity overflow")?;
        ids.insert(kind, raw, index as u32);
    }
    // Auto-trait ordering in the interner may follow process-local IDs.
    fn ordered(ty: Ty, ids: &IdMap, memo: &mut FxHashMap<Ty, Ty>) -> Ty {
        if let Some(&ordered) = memo.get(&ty) {
            return ordered;
        }
        let original = ty;
        let ty = ty.map_children(|child| ordered(child, ids, memo));
        if let TyKind::Dynamic(mut d) = ty.kind() {
            let mut autos = d.auto_traits.to_vec();
            autos.sort_unstable_by_key(|tr| {
                ids.get(IdKind::Trait, tr.trait_.into_raw())
                    .unwrap_or(u32::MAX)
            });
            d.auto_traits = crate::ty::object_auto_traits(&autos);
            let ty = Ty::new(TyKind::Dynamic(d));
            memo.insert(original, ty);
            return ty;
        }
        memo.insert(original, ty);
        ty
    }
    let mut e = Encoder::with_ids(ids);
    let ordered = ordered(canonical_ty, &e.ids, &mut FxHashMap::default());
    // Encode definitions before their parents. Nested metadata wrappers then
    // contain only completed type references, so the wire nesting budget does
    // not impose a stricter limit than the semantic type-depth budget.
    for ty in identity_postorder(ordered)? {
        ty.encode(&mut e).map_err(|error| error.0)?;
    }
    prefix.write(&e.into_bytes());
    Ok(prefix.into_bytes())
}

// Every type-valued field serialized by Ty, including fields omitted by the
// ordinary semantic child walk. Completion order also computes subtree heights,
// so sharing a child at different depths cannot hide an over-limit type.
fn identity_children(ty: Ty) -> Vec<Ty> {
    let mut children: Vec<_> = ty.children().collect();
    match ty.kind() {
        TyKind::Value(value) => children.push(value.ty),
        TyKind::Dynamic(object) => {
            for bound in object.auto_traits {
                children.extend_from_slice(bound.args);
            }
        }
        _ => {}
    }
    children
}

fn identity_postorder(root: Ty) -> Result<Vec<Ty>, &'static str> {
    let mut pending = vec![(root, false)];
    let mut depths = FxHashMap::<Ty, u32>::default();
    let mut ordered = Vec::new();
    while let Some((ty, complete)) = pending.pop() {
        if depths.contains_key(&ty) {
            continue;
        }
        let children = identity_children(ty);
        if !complete {
            pending.push((ty, true));
            pending.extend(children.into_iter().rev().map(|child| (child, false)));
            continue;
        }
        let mut depth = 1;
        for child in children {
            depth = depth.max(depths[&child] + 1);
        }
        if depth > flux_hir::flib::MAX_NESTING_DEPTH {
            return Err("type identity recursion limit reached");
        }
        depths.insert(ty, depth);
        ordered.push(ty);
    }
    Ok(ordered)
}

/// Fingerprints canonical structural type bytes using 128 bits of SHA-256.
pub fn stable_id(
    items: &ItemTree,
    hashes: &[(CrateId, [u8; 32])],
    canonical_ty: Ty,
) -> Result<u128, &'static str> {
    let digest = flux_hir::flib::digest(&stable_key(items, hashes, canonical_ty)?);
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    Ok(u128::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IntTy, Mutability};
    use flux_intern::Symbol;

    #[test]
    fn legal_semantic_depth_does_not_exhaust_wire_nesting() {
        let items = ItemTree::default();
        let mut ty = Ty::BOOL;
        for _ in 1..flux_hir::flib::MAX_NESTING_DEPTH {
            ty = Ty::tuple(&[ty]);
        }
        let key = stable_key(&items, &[], ty).unwrap();
        assert!(
            key.len() < 32 * flux_hir::flib::MAX_NESTING_DEPTH as usize,
            "type definitions must remain linear"
        );
        assert!(stable_id(&items, &[], ty).is_ok());
        let too_deep = Ty::tuple(&[ty]);
        assert_eq!(
            stable_key(&items, &[], too_deep),
            Err("type identity recursion limit reached")
        );
        // The same subtree is first reached shallowly, then through a deeper
        // branch. Its cached completion must still contribute its full height.
        assert_eq!(
            stable_key(&items, &[], Ty::tuple(&[Ty::BOOL, too_deep])),
            Err("type identity recursion limit reached")
        );
    }

    #[test]
    fn query_order_does_not_assign_runtime_identity() {
        let items = ItemTree::default();
        let first = TypeIds::default();
        let second = TypeIds::default();
        let a = Ty::tuple(&[Ty::BOOL, Ty::int(IntTy::U32)]);
        let b = Ty::raw_ptr(Mutability::Not, Ty::BOOL);
        let first_a = first.of(&items, a).unwrap();
        let first_b = first.of(&items, b).unwrap();
        let second_b = second.of(&items, b).unwrap();
        let second_a = second.of(&items, a).unwrap();
        assert_eq!(first_a, second_a);
        assert_eq!(first_b, second_b);
        assert_ne!(first_a, first_b);
    }

    #[test]
    fn crate_hash_and_local_identity_survive_graph_offsets() {
        fn graph(extra: bool) -> (ItemTree, Ty, CrateId) {
            let mut db = flux_hir::SourceDb::new();
            let mut sources = Vec::new();
            if extra {
                let root = db.add("/unrelated/lib.rs", "pub struct Other; fn unrelated() {}\n");
                sources.push(flux_hir::SourceCrate {
                    root,
                    name: Symbol::intern("unrelated"),
                    dependencies: vec![],
                });
            }
            let root = db.add("/packet/lib.rs", "pub struct Packet;\n");
            sources.push(flux_hir::SourceCrate {
                root,
                name: Symbol::intern("packet"),
                dependencies: vec![],
            });
            let hir = flux_hir::lower_sources(&mut db, &sources);
            assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
            let (id, item) = hir
                .items
                .structs
                .iter()
                .find(|(_, s)| {
                    s.name.as_str() == "Packet"
                        && hir.items.crates[hir.items.modules[s.loc.module].krate.0 as usize]
                            .name
                            .as_str()
                            == "packet"
                })
                .unwrap();
            let krate = hir.items.modules[item.loc.module].krate;
            let ty = Ty::adt(id, item.name, &[]);
            (hir.items, ty, krate)
        }
        let (a, ta, ka) = graph(false);
        let (b, tb, kb) = graph(true);
        assert_ne!(
            ta,
            tb,
            "test must exercise a shifted arena index: {:?} {:?} counts {} {}",
            ta.kind(),
            tb.kind(),
            a.structs.len(),
            b.structs.len()
        );
        let hash = [7; 32];
        let TyKind::Adt(adt, _) = ta.kind() else {
            panic!("expected test ADT");
        };
        assert_eq!(
            definition_key(&a, &[(ka, hash)], IdKind::Struct, adt.strukt.into_raw()).unwrap(),
            definition_key_with_ordinal(
                &a,
                &[(ka, hash)],
                IdKind::Struct,
                adt.strukt.into_raw(),
                0
            )
            .unwrap()
        );
        let ia = stable_id(&a, &[(ka, hash)], ta).unwrap();
        let ib = stable_id(&b, &[(kb, hash)], tb).unwrap();
        assert_eq!(ia, ib);
        assert_ne!(ia, stable_id(&a, &[(ka, [8; 32])], ta).unwrap());
    }

    #[test]
    fn macro_impl_definition_keys_ignore_definition_crate_indices() {
        fn graph(padded: bool) -> (ItemTree, flux_hir::FunctionId, CrateId) {
            let mut db = flux_hir::SourceDb::new();
            let mut sources = Vec::new();
            if padded {
                let root = db.add("/padding/lib.rs", "pub struct Padding;");
                sources.push(flux_hir::SourceCrate {
                    root,
                    name: Symbol::intern("padding"),
                    dependencies: vec![],
                });
            }
            let root = db.add(
                "/macro_impl/lib.rs",
                r#"
                pub trait Finish { fn finish(self) -> u32; }
                macro_rules! make {
                    () => {
                        pub struct Value;
                        impl $crate::Finish for Value { fn finish(self) -> u32 { 42 } }
                    };
                }
                make!();
            "#,
            );
            sources.push(flux_hir::SourceCrate {
                root,
                name: Symbol::intern("macro_impl"),
                dependencies: vec![],
            });
            let hir = flux_hir::lower_sources(&mut db, &sources);
            assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
            let krate = hir.items.main_crate();
            let function = hir
                .items
                .functions
                .iter()
                .find(|(_, f)| {
                    hir.items.modules[f.loc.module].krate == krate
                        && f.name.as_str() == "finish"
                        && matches!(f.owner, Some(flux_hir::Owner::Impl(_)))
                })
                .unwrap()
                .0;
            (hir.items, function, krate)
        }
        let (left, a, ka) = graph(false);
        let (right, b, kb) = graph(true);
        assert_ne!(
            left.function_path(&left.functions[a]),
            right.function_path(&right.functions[b])
        );
        assert_eq!(
            definition_key(&left, &[(ka, [7; 32])], IdKind::Function, a.into_raw()).unwrap(),
            definition_key(&right, &[(kb, [7; 32])], IdKind::Function, b.into_raw()).unwrap(),
        );
    }

    #[test]
    fn cached_definition_keys_match_uncached() {
        let mut db = flux_hir::SourceDb::new();
        let mut sources = Vec::new();
        for (path, name, text) in [
            (
                "/one/lib.rs",
                "one",
                "pub struct A; pub trait T {} fn f() {} const C: u8 = 0;\n",
            ),
            (
                "/two/lib.rs",
                "two",
                "pub struct B; mod m { pub struct A; fn f() {} } fn g() {} static S: u8 = 1;\n",
            ),
        ] {
            let root = db.add(path, text);
            sources.push(flux_hir::SourceCrate {
                root,
                name: Symbol::intern(name),
                dependencies: vec![],
            });
        }
        let hir = flux_hir::lower_sources(&mut db, &sources);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let items = &hir.items;
        let hashed = [(CrateId(1), [3; 32])];
        for hashes in [&[][..], &hashed[..]] {
            let mut keys = DefinitionKeys::new(items, hashes);
            let mut definitions = Vec::new();
            definitions.extend(
                items
                    .structs
                    .iter()
                    .map(|(id, _)| (IdKind::Struct, id.into_raw())),
            );
            definitions.extend(
                items
                    .traits
                    .iter()
                    .map(|(id, _)| (IdKind::Trait, id.into_raw())),
            );
            definitions
                .extend((items.functions.iter()).map(|(id, _)| (IdKind::Function, id.into_raw())));
            definitions.extend(
                items
                    .consts
                    .iter()
                    .map(|(id, _)| (IdKind::Const, id.into_raw())),
            );
            // Twice: computed, then remembered.
            for (kind, raw) in definitions.iter().chain(&definitions) {
                assert_eq!(
                    keys.key(*kind, *raw).unwrap(),
                    definition_key(items, hashes, *kind, *raw).unwrap(),
                    "{kind:?} {raw}"
                );
            }
            for (id, s) in items.structs.iter() {
                let ty = Ty::tuple(&[Ty::adt(id, s.name, &[]), Ty::BOOL]);
                assert_eq!(
                    keys.stable_key(ty).unwrap(),
                    stable_key(items, hashes, ty).unwrap()
                );
            }
        }
    }

    #[test]
    fn fingerprint_collision_is_reported() {
        let items = ItemTree::default();
        let ids = TypeIds::default();
        let fingerprint = stable_id(&items, &[], Ty::BOOL).unwrap();
        ids.0
            .lock()
            .unwrap()
            .insert(Ty::int(IntTy::U8), fingerprint);
        assert_eq!(
            ids.of(&items, Ty::BOOL),
            Err("type identity fingerprint collision")
        );
    }
}

#[cfg(test)]
mod process_tests {
    use super::*;
    use crate::{FnPtrRegions, FnRegion, FnSigTy, Mutability};

    #[test]
    fn identity_matches_fresh_process() {
        let items = ItemTree::default();
        let ty = Ty::tuple(&[Ty::BOOL, Ty::raw_ptr(Mutability::Mut, Ty::CHAR)]);
        let id = stable_id(&items, &[], ty).unwrap();
        if let Ok(expected) = std::env::var("FLUX_TYPE_ID_CHILD_EXPECTED") {
            assert_eq!(id.to_string(), expected);
            return;
        }
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "type_identity::process_tests::identity_matches_fresh_process",
            ])
            .env("FLUX_TYPE_ID_CHILD_EXPECTED", id.to_string())
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
    }

    #[test]
    fn callable_binders_contribute_to_identity() {
        let items = ItemTree::default();
        let reference = Ty::new(TyKind::Ref(Mutability::Not, Ty::BOOL));
        let sig = |rows: &[Vec<FnRegion>]| {
            Ty::new(TyKind::FnPtr(FnSigTy {
                inputs_output: Ty::list(&[reference, reference]),
                is_unsafe: false,
                abi: flux_hir::Abi::Rust,
                regions: Some(FnPtrRegions::intern(1, 0, rows, &[])),
            }))
        };
        let bound = sig(&[vec![FnRegion::Bound(0)], vec![FnRegion::Bound(0)]]);
        let static_return = sig(&[vec![FnRegion::Bound(0)], vec![FnRegion::Static]]);
        assert_ne!(
            stable_id(&items, &[], bound).unwrap(),
            stable_id(&items, &[], static_return).unwrap()
        );
    }
}

#[cfg(test)]
mod source_identity_tests {
    use super::*;
    use flux_intern::Symbol;

    #[test]
    fn same_name_crates_in_one_directory_keep_distinct_identities() {
        let mut db = flux_hir::SourceDb::new();
        let first = db.add("/same/first.rs", "pub struct SourceIdentity;\n");
        let second = db.add("/same/second.rs", "pub struct SourceIdentity;\n");
        let sources = [
            flux_hir::SourceCrate {
                root: first,
                name: Symbol::intern("same_name"),
                dependencies: vec![],
            },
            flux_hir::SourceCrate {
                root: second,
                name: Symbol::intern("same_name"),
                dependencies: vec![],
            },
        ];
        let hir = flux_hir::lower_sources(&mut db, &sources);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let identities: Vec<_> = hir
            .items
            .structs
            .iter()
            .filter(|(_, s)| s.name.as_str() == "SourceIdentity")
            .map(|(id, s)| stable_id(&hir.items, &[], Ty::adt(id, s.name, &[])).unwrap())
            .collect();
        assert_eq!(identities.len(), 2);
        assert_ne!(identities[0], identities[1]);
    }
}

#[cfg(test)]
mod isolation_tests {
    use super::*;
    #[test]
    fn extension_graphs_do_not_share_hash_configuration() {
        let source = TypeIds::default();
        source.set_crate_hashes(&[(CrateId(0), [1; 32])]);
        let fork = source.fork();
        let same_graph = source.clone();
        fork.set_crate_hashes(&[(CrateId(0), [2; 32])]);
        assert_eq!(
            source.1.lock().unwrap().as_slice(),
            &[(CrateId(0), [1; 32])]
        );
        same_graph.set_crate_hashes(&[(CrateId(0), [3; 32])]);
        assert_eq!(
            source.1.lock().unwrap().as_slice(),
            &[(CrateId(0), [3; 32])]
        );
        assert_eq!(fork.1.lock().unwrap().as_slice(), &[(CrateId(0), [2; 32])]);
    }
}
