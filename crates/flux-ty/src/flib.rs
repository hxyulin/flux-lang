//! Checked dependency signatures and ownership for source-only type checking.

use flux_hir::flib::{CrateIndices, Error};
use flux_hir::{ConstId, CrateId, FunctionId, ItemCounts, ItemTree};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{ArrayLens, Diagnostic, Signatures};

/// A checked dependency identity and the ownership lists produced by its loader.
#[derive(Debug, Clone, Copy)]
pub struct LoadedCrate<'a> {
    pub krate: CrateId,
    pub hash: [u8; 32],
    pub indices: &'a CrateIndices,
}

/// Type information loaded before the compilation's source crate is collected.
/// Imported bodies have no inference result; their checked MIR is supplied by
/// the artifact body provider instead.
#[derive(Debug, Clone)]
pub struct LoadedTypes {
    signatures: Signatures,
    source_counts: ItemCounts,
    hashes: FxHashMap<CrateId, [u8; 32]>,
    functions: FxHashSet<FunctionId>,
    consts: FxHashSet<ConstId>,
}

impl LoadedTypes {
    /// Captures dependency boundaries after artifact loading and before source collection.
    /// Rejects duplicate identities and missing signature/region rows.
    pub fn new(
        items: &ItemTree,
        signatures: Signatures,
        loaded: &[(CrateId, [u8; 32])],
    ) -> Result<Self, Error> {
        Self::collected(items, signatures, loaded, false)
    }

    /// Accepts an authenticated built-in prefix and ordinary decoded dependencies.
    /// The HIR suite decoder is the only producer of the required trusted marker.
    pub fn with_builtin_prefix(
        items: &ItemTree,
        signatures: Signatures,
        loaded: &[(CrateId, [u8; 32])],
    ) -> Result<Self, Error> {
        if !items.has_artifact_builtins() {
            return Err(Error("expected authenticated built-in metadata"));
        }
        let mut signatures = signatures;
        signatures.freeze();
        signatures.type_ids = signatures.type_ids.fork();
        Self::collected(items, signatures, loaded, true)
    }

    /// Reuses loader-produced ownership instead of rescanning the dependency graph.
    pub fn from_metadata(
        items: &ItemTree,
        signatures: Signatures,
        loaded: &[LoadedCrate<'_>],
    ) -> Result<Self, Error> {
        let builtins = items.has_artifact_builtins();
        Self::checked(items, signatures, loaded, builtins)
    }

    fn collected(
        items: &ItemTree,
        signatures: Signatures,
        loaded: &[(CrateId, [u8; 32])],
        builtins: bool,
    ) -> Result<Self, Error> {
        let indices: Vec<_> = loaded
            .iter()
            .map(|&(id, _)| CrateIndices::collect(items, id))
            .collect();
        let loaded: Vec<_> = loaded
            .iter()
            .zip(&indices)
            .map(|(&(krate, hash), indices)| LoadedCrate {
                krate,
                hash,
                indices,
            })
            .collect();
        Self::checked(items, signatures, &loaded, builtins)
    }

    fn checked(
        items: &ItemTree,
        mut signatures: Signatures,
        loaded: &[LoadedCrate<'_>],
        builtins: bool,
    ) -> Result<Self, Error> {
        let mut hashes = FxHashMap::default();
        let mut seen_hashes = FxHashSet::default();
        let mut functions = FxHashSet::default();
        let mut consts = FxHashSet::default();
        if builtins {
            for (index, data) in items.crates.iter().enumerate() {
                if data.root.into_raw() < items.core.modules {
                    let krate = CrateId(index as u32);
                    if !loaded.iter().any(|d| d.krate == krate) {
                        return Err(Error("missing built-in crate identity"));
                    }
                }
            }
        }
        for dependency in loaded {
            let LoadedCrate {
                krate,
                hash,
                indices,
            } = *dependency;
            let data = items
                .crates
                .get(krate.0 as usize)
                .ok_or(Error("invalid loaded crate identity"))?;
            let root = items
                .modules
                .get(data.root)
                .ok_or(Error("invalid loaded crate root"))?;
            if root.krate != krate {
                return Err(Error("invalid loaded crate root ownership"));
            }
            let builtin = builtins && data.root.into_raw() < items.core.modules;
            if !builtin && (data.is_core || items.is_std(data.root)) {
                return Err(Error("embedded crates cannot be imported as user flibs"));
            }
            if hashes.insert(krate, hash).is_some() || !seen_hashes.insert(hash) {
                return Err(Error("duplicate loaded crate identity"));
            }
            for &raw in &indices.functions {
                let id = FunctionId::from_raw(raw);
                if signatures.fns.get(id).is_none() || signatures.regions.fns.get(id).is_none() {
                    return Err(Error("missing loaded function signature"));
                }
                let function = &items.functions[id];
                if !builtin
                    && (function.intrinsic.is_some() || signatures.sized_dispatch.contains_key(&id))
                {
                    return Err(Error("untrusted loaded intrinsic"));
                }
                for index in 0..function.opaques.len() as u32 {
                    if function.has_body && !signatures.hidden.contains_key(&(id, index)) {
                        return Err(Error("missing loaded opaque hidden type"));
                    }
                }
                functions.insert(id);
            }
            for &raw in &indices.consts {
                let id = ConstId::from_raw(raw);
                if signatures.consts.get(id).is_none()
                    || signatures.regions.consts.get(id).is_none()
                {
                    return Err(Error("missing loaded constant signature"));
                }
                consts.insert(id);
            }
            for &raw in &indices.structs {
                let id = flux_hir::StructId::from_raw(raw);
                if signatures.structs.get(id).is_none()
                    || signatures.regions.structs.get(id).is_none()
                {
                    return Err(Error("missing loaded struct signature"));
                }
            }
            for &raw in &indices.traits {
                let id = flux_hir::TraitId::from_raw(raw);
                if signatures.traits.get(id).is_none()
                    || signatures.regions.traits.get(id).is_none()
                {
                    return Err(Error("missing loaded trait signature"));
                }
            }
            for &raw in &indices.impls {
                let id = flux_hir::ImplId::from_raw(raw);
                if signatures.impls.get(id).is_none() || signatures.regions.impls.get(id).is_none()
                {
                    return Err(Error("missing loaded impl signature"));
                }
            }
        }
        if builtins
            && (functions
                .iter()
                .filter(|id| id.into_raw() < items.core.functions)
                .count()
                != items.core.functions as usize
                || consts
                    .iter()
                    .filter(|id| id.into_raw() < items.core.consts)
                    .count()
                    != items.core.consts as usize)
        {
            return Err(Error("incomplete built-in signature prefix"));
        }
        signatures.freeze();
        Ok(Self {
            signatures,
            source_counts: items.counts(),
            hashes,
            functions,
            consts,
        })
    }

    /// The checked dependency tables, before any source signatures are added.
    pub fn signatures(&self) -> &Signatures {
        &self.signatures
    }

    /// Arena boundaries immediately before the source crate is collected.
    pub fn source_counts(&self) -> ItemCounts {
        self.source_counts
    }

    /// The exact content identity supplied for one loaded crate.
    pub fn hash(&self, krate: CrateId) -> Option<[u8; 32]> {
        self.hashes.get(&krate).copied()
    }

    /// Whether a function already has checked dependency metadata.
    pub fn owns_function(&self, function: FunctionId) -> bool {
        self.functions.contains(&function)
    }

    /// Whether a constant already has checked dependency metadata.
    pub fn owns_const(&self, konst: ConstId) -> bool {
        self.consts.contains(&konst)
    }

    /// Resolves only source signatures while preserving checked dependency rows.
    pub fn extend_source_signatures(
        &self,
        items: &ItemTree,
        lens: Option<&ArrayLens<'_>>,
    ) -> (Signatures, Vec<Diagnostic>) {
        crate::signatures_from(
            items,
            self.signatures.fork_for_extension(),
            self.source_counts,
            lens,
        )
    }
}

/// Chooses the checked owner before inference can ask for a source body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyOrigin {
    Library,
    Imported,
    Source,
}

/// Embedded library ownership takes precedence over user artifact ownership.
pub fn body_origin(
    library: Option<&crate::LibraryTypes>,
    loaded: Option<&LoadedTypes>,
    owner: crate::ClosureOwner,
) -> BodyOrigin {
    if library.is_some_and(|library| library.owns(owner)) {
        return BodyOrigin::Library;
    }
    if loaded.is_some_and(|loaded| match owner {
        crate::ClosureOwner::Function(f) => loaded.owns_function(f),
        crate::ClosureOwner::Const(c) => loaded.owns_const(c),
    }) {
        return BodyOrigin::Imported;
    }
    BodyOrigin::Source
}
