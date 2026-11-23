//! Checked dependency bodies supplied without dependency HIR or inference.

use std::sync::{Arc, OnceLock};

use flux_hir::flib::{Decode, Decoder, Encode, Encoder, Error, IdKind};
use flux_hir::{ConstId, FunctionId, ItemCounts, ItemTree};
use flux_ty::{ClosureId, ClosureKind, ClosureOwner};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::mono::{ValueReference, visit_value_references};
use crate::{Body, FileMir};

/// One provider contribution shared by source checking and code generation.
#[derive(Debug, Default)]
pub struct RequiredRoots {
    pub functions: Vec<FunctionId>,
    pub instances: Vec<flux_ty::Instance>,
}

/// Body facts that do not depend on the consuming crate's trait impls.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct FunctionSummary {
    callees: Vec<FunctionId>,
    promotions: Vec<FunctionId>,
    coroutines: Vec<FunctionId>,
}

impl FunctionSummary {
    /// Summarizes the same large function tables that use indexed MIR.
    pub fn collect(mir: &FileMir, items: &ItemTree, functions: &[u32]) -> Option<Self> {
        if functions.len() < crate::flib::MIN_INDEXED_FUNCTIONS {
            return None;
        }
        let bodies: Vec<_> = functions
            .iter()
            .copied()
            .map(FunctionId::from_raw)
            .filter_map(|id| mir.bodies.get(id).map(|body| (id, body)))
            .collect();
        if bodies.len() < crate::flib::MIN_INDEXED_FUNCTIONS {
            return None;
        }
        let mut summary = Self::default();
        let mut callees = FxHashSet::default();
        for (id, body) in bodies {
            callees.extend(body.callees());
            callees.extend(body.object_methods(items));
            let mut has_vtable = false;
            visit_value_references(&body.promoted, None, |reference| match reference {
                ValueReference::Function(instance) => {
                    callees.insert(instance.function);
                }
                ValueReference::Vtable(..) => has_vtable = true,
            });
            // The consumer processes every stored static value already. Only
            // vtable promotions still need the original value and its types.
            if has_vtable {
                summary.promotions.push(id);
            }
            if crate::auto_traits::constructs_coroutine(items, body) {
                summary.coroutines.push(id);
            }
        }
        summary.callees.extend(callees);
        summary.callees.sort_unstable();
        Some(summary)
    }

    /// Writes crate-local references with the artifact's item mapping.
    pub fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.callees.encode(e)?;
        self.promotions.encode(e)?;
        self.coroutines.encode(e)
    }

    /// Rejects references without a declaration or outside the summary's crate.
    pub fn decode(
        d: &mut Decoder<'_>,
        items: &ItemTree,
        mir: &FileMir,
        sigs: &flux_ty::Signatures,
        functions: &[u32],
    ) -> Result<Self, Error> {
        let callees = Vec::<FunctionId>::decode(d)?;
        let promotions = Vec::<FunctionId>::decode(d)?;
        let coroutines = Vec::<FunctionId>::decode(d)?;
        if callees.iter().any(|&id| items.functions.get(id).is_none()) {
            return Err(Error::new("invalid imported function summary"));
        }
        for &id in promotions.iter().chain(&coroutines) {
            if !d.ids.contains_owned(IdKind::Function, id.into_raw()) {
                return Err(Error::new("invalid imported function summary owner"));
            }
            // The summary is read after closure and constant records, so a
            // demanded function can check its closure shims here.
            let Some(body) = mir.function_body(items, sigs, id)? else {
                return Err(Error::new("invalid imported function summary owner"));
            };
            if (promotions.contains(&id) && body.promoted.is_empty())
                || (coroutines.contains(&id)
                    && !crate::auto_traits::constructs_coroutine(items, body))
            {
                return Err(Error::new("invalid imported function summary"));
            }
        }
        let mut summary = Self {
            callees,
            promotions,
            coroutines,
        };
        if cfg!(debug_assertions) {
            // A dependency slot can be assigned different process IDs when
            // the artifact is read as part of another graph.
            summary.callees.sort_unstable();
            if Self::collect(mir, items, functions).as_ref() != Some(&summary) {
                return Err(Error::new("inconsistent imported function summary"));
            }
        }
        Ok(summary)
    }
}

/// Summaries and the functions that still need to be inspected in legacy archives.
#[derive(Debug, Default)]
pub struct ImportedFunctionSummaries {
    summarized: FunctionSummary,
    legacy: Vec<FunctionId>,
}

impl ImportedFunctionSummaries {
    /// Records one archive in the same order as its function bodies.
    pub fn append(&mut self, summary: Option<FunctionSummary>, functions: &[u32]) {
        if let Some(summary) = summary {
            self.summarized.callees.extend(summary.callees);
            self.summarized.promotions.extend(summary.promotions);
            self.summarized.coroutines.extend(summary.coroutines);
        } else {
            self.legacy
                .extend(functions.iter().copied().map(FunctionId::from_raw));
        }
    }
}

/// A checked MIR snapshot loaded before the source crate is collected.
#[derive(Debug)]
pub struct ImportedMir {
    mir: FileMir,
    builtins: Option<Arc<FileMir>>,
    closure_keys: OnceLock<FxHashMap<ClosureOwner, Vec<(ClosureId, ClosureKind)>>>,
    source_counts: ItemCounts,
    required_instances: Vec<flux_ty::Instance>,
    function_summaries: Option<ImportedFunctionSummaries>,
}

impl ImportedMir {
    /// Keeps decoded dependency bodies separate from bodies that still need checking.
    pub fn new(mir: FileMir, source_counts: ItemCounts) -> Result<Self, Error> {
        if !mir.diagnostics.is_empty() || !mir.bugs.is_empty() {
            return Err(Error::new("imported MIR contains errors"));
        }
        if mir
            .bodies
            .iter()
            .any(|(id, _)| id.into_raw() >= source_counts.functions)
            || mir
                .consts
                .iter()
                .any(|(id, _)| id.into_raw() >= source_counts.consts)
            || mir
                .const_values
                .deferred
                .keys()
                .any(|id| id.into_raw() >= source_counts.consts)
            || mir.closures.keys().any(|(id, _)| match id.owner {
                ClosureOwner::Function(f) => f.into_raw() >= source_counts.functions,
                ClosureOwner::Const(c) => c.into_raw() >= source_counts.consts,
            })
        {
            return Err(Error::new("imported MIR exceeds source boundary"));
        }
        Ok(Self {
            mir,
            builtins: None,
            closure_keys: OnceLock::new(),
            source_counts,
            required_instances: Vec::new(),
            function_summaries: None,
        })
    }

    /// Shares an authenticated whole-prefix suite without copying its functions.
    /// The mutable MIR contains user imports and the built-in constant tables;
    /// suite function templates are copied only when a compilation reaches them.
    pub fn with_builtin_prefix(
        items: &flux_hir::ItemTree,
        mir: FileMir,
        suite: Arc<FileMir>,
    ) -> Result<Self, Error> {
        if !items.has_artifact_builtins() {
            return Err(Error::new("expected authenticated built-in metadata"));
        }
        if mir.panic_strategy != suite.panic_strategy {
            return Err(Error::new("panic strategy mismatch in built-in MIR"));
        }
        let mut imported = Self::new(mir, items.counts())?;
        if suite
            .bodies
            .iter()
            .any(|(id, _)| id.into_raw() >= items.core.functions)
            || suite
                .consts
                .iter()
                .any(|(id, _)| id.into_raw() >= items.core.consts)
            || suite.closures.keys().any(|(id, _)| match id.owner {
                ClosureOwner::Function(f) => f.into_raw() >= items.core.functions,
                ClosureOwner::Const(c) => c.into_raw() >= items.core.consts,
            })
            || suite
                .const_values
                .items
                .iter()
                .any(|(id, _)| id.into_raw() >= items.core.consts)
            || suite
                .const_values
                .instances
                .keys()
                .any(|key| key.konst.into_raw() >= items.core.consts)
            || suite
                .discriminants
                .iter()
                .any(|(id, _)| id.into_raw() >= items.core.structs)
            || !suite.diagnostics.is_empty()
            || !suite.bugs.is_empty()
        {
            return Err(Error::new("invalid built-in MIR prefix"));
        }
        for (id, function) in items.functions.iter().take(items.core.functions as usize) {
            if function.has_body && suite.bodies.get(id).is_none() {
                return Err(Error::new("missing built-in function MIR"));
            }
        }
        for (id, konst) in items.consts.iter().take(items.core.consts as usize) {
            if konst.has_body && suite.consts.get(id).is_none() {
                return Err(Error::new("missing built-in constant MIR"));
            }
        }
        if imported
            .mir
            .bodies
            .iter()
            .any(|(id, _)| id.into_raw() < items.core.functions)
            || imported
                .mir
                .consts
                .iter()
                .any(|(id, _)| id.into_raw() < items.core.consts)
            || imported.mir.closures.keys().any(|(id, _)| match id.owner {
                ClosureOwner::Function(f) => f.into_raw() < items.core.functions,
                ClosureOwner::Const(c) => c.into_raw() < items.core.consts,
            })
        {
            return Err(Error::new("duplicate built-in MIR provider"));
        }
        for (id, body) in suite.consts.iter() {
            imported.mir.consts.insert(id, body.clone());
        }
        for (&key, body) in &suite.closures {
            if matches!(key.0.owner, ClosureOwner::Const(_)) {
                imported.mir.closures.insert(key, body.clone());
            }
        }
        for (id, value) in suite.const_values.items.iter() {
            if imported.mir.const_values.items.get(id).is_some() {
                return Err(Error::new("duplicate built-in constant value"));
            }
            imported.mir.const_values.items.insert(id, value.clone());
        }
        for (&key, value) in &suite.const_values.instances {
            if imported
                .mir
                .const_values
                .instances
                .insert(key, value.clone())
                .is_some()
            {
                return Err(Error::new("duplicate built-in constant instance"));
            }
        }
        for (id, values) in suite.discriminants.iter() {
            if imported.mir.discriminants.get(id).is_some() {
                return Err(Error::new("duplicate built-in enum discriminants"));
            }
            imported.mir.discriminants.insert(id, values.clone());
        }
        imported.builtins = Some(suite);
        Ok(imported)
    }

    /// Adds concrete dependencies recorded by the producing object's collector.
    pub fn with_required_instances(
        mut self,
        instances: Vec<flux_ty::Instance>,
    ) -> Result<Self, Error> {
        if instances.iter().any(|instance| {
            instance.function.into_raw() >= self.source_counts.functions
                || instance
                    .args
                    .iter()
                    .any(|ty| ty.has_params() || ty.has_infer() || ty.has_error())
        }) {
            return Err(Error::new("invalid imported object instance"));
        }
        self.required_instances = instances;
        Ok(self)
    }

    /// Uses the per-archive summaries when collecting dependency roots.
    pub fn with_function_summaries(mut self, summaries: ImportedFunctionSummaries) -> Self {
        self.function_summaries = Some(summaries);
        self
    }

    /// Keeps the checked builtin templates accessible to separate code
    /// generation without cloning them into the consumer's MIR.
    pub fn share_builtin_templates(mut self) -> Self {
        self.mir.deferred_functions.builtin_prefix = self.builtins.clone();
        self
    }

    /// Supplies checked dependency MIR and explicit object requirements for codegen.
    pub fn with_codegen_inputs<T>(
        &mut self,
        f: impl FnOnce(&FileMir, &[flux_ty::Instance]) -> T,
    ) -> T {
        let previous = std::mem::replace(
            &mut self.mir.deferred_functions.builtin_prefix,
            self.builtins.clone(),
        );
        let output = f(&self.mir, &self.required_instances);
        self.mir.deferred_functions.builtin_prefix = previous;
        output
    }

    fn unsummarized_functions(&self) -> impl Iterator<Item = (FunctionId, &Body)> {
        self.function_summaries
            .iter()
            .flat_map(|summaries| &summaries.legacy)
            .filter_map(|&id| self.mir.bodies.get(id).map(|body| (id, body)))
            .chain(
                self.function_summaries
                    .is_none()
                    .then(|| self.mir.bodies.iter())
                    .into_iter()
                    .flatten(),
            )
    }

    /// The boundary captured before adding the source crate.
    pub fn source_counts(&self) -> ItemCounts {
        self.source_counts
    }

    /// A dependency function's already checked generic template.
    pub fn function(
        &self,
        items: &ItemTree,
        sigs: &flux_ty::Signatures,
        id: FunctionId,
    ) -> Result<Option<&Body>, Error> {
        Ok(self
            .mir
            .function_body(items, sigs, id)?
            .or_else(|| self.builtins.as_ref()?.bodies.get(id)))
    }

    /// A dependency constant's checked initializer.
    pub fn konst(&self, id: ConstId) -> Option<&Body> {
        self.mir
            .consts
            .get(id)
            .or_else(|| self.builtins.as_ref()?.consts.get(id))
    }

    /// Checked closure bodies and callable shims belonging to one dependency owner.
    pub fn closures(
        &self,
        owner: ClosureOwner,
    ) -> impl Iterator<Item = ((ClosureId, ClosureKind), &Body)> {
        let keys = self.closure_keys.get_or_init(|| {
            let mut by_owner = FxHashMap::<_, Vec<_>>::default();
            let builtin_functions = self
                .builtins
                .iter()
                .flat_map(|suite| suite.closures.keys())
                .filter(|(id, _)| matches!(id.owner, ClosureOwner::Function(_)));
            for &key in self.mir.closures.keys().chain(builtin_functions) {
                by_owner.entry(key.0.owner).or_default().push(key);
            }
            by_owner
        });
        keys.get(&owner).into_iter().flatten().map(|&key| {
            let body = self
                .mir
                .closures
                .get(&key)
                .or_else(|| self.builtins.as_ref()?.closures.get(&key))
                .expect("indexed closure body exists");
            (key, body)
        })
    }

    /// Owners whose checked templates construct a private source coroutine.
    pub(crate) fn coroutine_owners(
        &self,
        items: &flux_hir::ItemTree,
    ) -> rustc_hash::FxHashSet<ClosureOwner> {
        let mut owners = rustc_hash::FxHashSet::default();
        if let Some(summaries) = &self.function_summaries {
            owners.extend(
                summaries
                    .summarized
                    .coroutines
                    .iter()
                    .copied()
                    .map(ClosureOwner::Function),
            );
        }
        for (id, body) in self
            .unsummarized_functions()
            .chain(self.builtins.iter().flat_map(|suite| suite.bodies.iter()))
        {
            if crate::auto_traits::constructs_coroutine(items, body) {
                owners.insert(ClosureOwner::Function(id));
            }
        }
        for mir in std::iter::once(&self.mir).chain(self.builtins.iter().map(|mir| mir.as_ref())) {
            for (id, body) in mir.consts.iter() {
                if crate::auto_traits::constructs_coroutine(items, body) {
                    owners.insert(ClosureOwner::Const(id));
                }
            }
            for ((id, _), body) in &mir.closures {
                if crate::auto_traits::constructs_coroutine(items, body) {
                    owners.insert(id.owner);
                }
            }
        }
        owners
    }

    /// Values and bodies needed by constant evaluation and instance collection.
    pub fn mir(&self) -> &FileMir {
        &self.mir
    }

    /// Functions referenced by imported bodies, promotions and constant allocations.
    /// Source checking uses these roots to obtain any embedded-library bodies
    /// needed by the imported objects without inferring dependency bodies.
    pub fn required_functions(
        &self,
        items: &flux_hir::ItemTree,
        sigs: &flux_ty::Signatures,
    ) -> RequiredRoots {
        let mut referenced: FxHashSet<_> = self
            .required_instances
            .iter()
            .map(|instance| instance.function)
            .collect();
        let mut values = Vec::new();
        if let Some(summaries) = &self.function_summaries {
            referenced.extend(summaries.summarized.callees.iter().copied());
            for &id in &summaries.summarized.promotions {
                if let Ok(Some(body)) = self.mir.function_body(items, sigs, id) {
                    values.extend(body.promoted.iter());
                }
            }
        }
        for body in self
            .unsummarized_functions()
            .map(|(_, body)| body)
            .chain(self.mir.consts.iter().map(|(_, body)| body))
            .chain(self.mir.closures.values())
        {
            referenced.extend(body.callees());
            referenced.extend(body.object_methods(items));
            values.extend(body.promoted.iter());
        }
        values.extend(self.mir.const_values.items.iter().map(|(_, value)| value));
        values.extend(self.mir.const_values.instances.values());
        let mut instances = Vec::new();
        for value in values {
            crate::mono::value_functions(
                value,
                &self.mir.const_values,
                items,
                sigs,
                &mut instances,
            );
            referenced.extend(instances.drain(..).map(|instance| instance.function));
        }
        // Every occurrence of a callee expands to the same possible targets
        // in this compilation, including the same set of trait impls.
        let mut roots = FxHashSet::default();
        for function in referenced {
            roots.extend(flux_ty::call_targets(items, function));
        }
        let mut functions: Vec<_> = roots.into_iter().collect();
        functions.sort_unstable();
        RequiredRoots {
            functions,
            instances: self.required_instances.clone(),
        }
    }

    /// Compatibility for callers retaining a borrowed provider after MIR construction.
    pub(crate) fn copy_bodies_into(&self, target: &mut FileMir) {
        assert_eq!(
            self.mir.panic_strategy, target.panic_strategy,
            "checked dependency strategy"
        );
        target
            .deferred_functions
            .builtin_prefix
            .clone_from(&self.mir.deferred_functions.builtin_prefix);
        for (id, body) in self.mir.bodies.iter() {
            target.bodies.insert(id, body.clone());
        }
        for (id, body) in self.mir.consts.iter() {
            target.consts.insert(id, body.clone());
        }
        for (&key, body) in &self.mir.closures {
            target.closures.insert(key, body.clone());
        }
    }

    /// Code generation needs the full graph; moving it here avoids copies while checking.
    pub fn finish(self, source: FileMir) -> FileMir {
        assert_eq!(
            self.mir.panic_strategy, source.panic_strategy,
            "checked dependency strategy"
        );
        fn append<T, V>(base: &mut flux_arena::ArenaMap<T, V>, source: flux_arena::ArenaMap<T, V>) {
            for (id, value) in source.into_entries() {
                base.insert(id, value);
            }
        }
        let mut mir = self.mir;
        append(&mut mir.bodies, source.bodies);
        append(&mut mir.consts, source.consts);
        mir.closures.extend(source.closures);
        append(&mut mir.const_values.items, source.const_values.items);
        mir.const_values
            .instances
            .extend(source.const_values.instances);
        append(&mut mir.discriminants, source.discriminants);
        mir.diagnostics = source.diagnostics;
        mir.bugs = source.bugs;
        mir
    }

    /// Copies checked dependency MIR into a fresh compilation's evaluation tables.
    /// Existing entries are errors rather than silently replacing checked bodies.
    pub fn seed_into(&self, target: &mut FileMir) -> Result<(), Error> {
        if self.mir.panic_strategy != target.panic_strategy {
            return Err(Error::new(
                "panic strategy mismatch in checked dependency MIR",
            ));
        }
        if self
            .mir
            .bodies
            .iter()
            .any(|(id, _)| target.bodies.get(id).is_some())
            || self
                .mir
                .consts
                .iter()
                .any(|(id, _)| target.consts.get(id).is_some())
            || self
                .mir
                .closures
                .keys()
                .any(|key| target.closures.contains_key(key))
            || self
                .mir
                .const_values
                .items
                .iter()
                .map(|(id, _)| id)
                .chain(self.mir.const_values.deferred.keys())
                .any(|id| target.const_values.contains_item(id))
            || self
                .mir
                .const_values
                .instances
                .keys()
                .any(|key| target.const_values.instances.contains_key(key))
            || self
                .mir
                .discriminants
                .iter()
                .any(|(id, _)| target.discriminants.get(id).is_some())
        {
            return Err(Error::new("duplicate checked dependency MIR"));
        }
        for (id, body) in self.mir.bodies.iter() {
            target.bodies.insert(id, body.clone());
        }
        for (id, body) in self.mir.consts.iter() {
            target.consts.insert(id, body.clone());
        }
        for (&key, body) in &self.mir.closures {
            target.closures.insert(key, body.clone());
        }
        for (id, value) in self.mir.const_values.items.iter() {
            target.const_values.items.insert(id, value.clone());
        }
        target
            .const_values
            .deferred
            .extend(&self.mir.const_values.deferred);
        for (&key, value) in &self.mir.const_values.instances {
            target.const_values.instances.insert(key, value.clone());
        }
        for (id, values) in self.mir.discriminants.iter() {
            target.discriminants.insert(id, values.clone());
        }
        Ok(())
    }
}

/// The checked provider for an owner, before falling back to source MIR.
pub(crate) enum SharedMir<'a> {
    Library(&'static crate::library::LibraryMir),
    Imported(&'a ImportedMir),
}

impl ImportedMir {
    /// Whether an owner belongs to a decoded user crate, excluding the core prefix.
    pub fn owns(&self, owner: ClosureOwner, core: ItemCounts) -> bool {
        let core = if self.builtins.is_some() {
            ItemCounts::default()
        } else {
            core
        };
        match owner {
            ClosureOwner::Function(id) => {
                (core.functions..self.source_counts.functions).contains(&id.into_raw())
            }
            ClosureOwner::Const(id) => {
                (core.consts..self.source_counts.consts).contains(&id.into_raw())
            }
        }
    }

    pub(crate) fn built(
        &self,
        items: &ItemTree,
        sigs: &flux_ty::Signatures,
        owner: ClosureOwner,
    ) -> Option<crate::build::Built> {
        let body = match owner {
            ClosureOwner::Function(id) => self.function(items, sigs, id).ok().flatten(),
            ClosureOwner::Const(id) => self.konst(id),
        }?;
        Some(crate::build::Built {
            body: body.clone(),
            closures: self
                .closures(owner)
                .map(|((id, kind), body)| crate::build::BuiltClosure {
                    id,
                    kind,
                    body: body.clone(),
                    shims: Vec::new(),
                })
                .collect(),
            closure_errors: Vec::new(),
            errors: Vec::new(),
        })
    }
}

impl<'a> SharedMir<'a> {
    pub(crate) fn of(
        hir: &flux_hir::Hir,
        imported: Option<&'a ImportedMir>,
        owner: ClosureOwner,
    ) -> Option<Self> {
        Self::of_with_strategy(
            hir,
            imported,
            owner,
            imported.map_or(crate::PanicStrategy::Abort, |imported| {
                imported.mir.panic_strategy
            }),
        )
    }

    pub(crate) fn of_with_strategy(
        hir: &flux_hir::Hir,
        imported: Option<&'a ImportedMir>,
        owner: ClosureOwner,
        strategy: crate::PanicStrategy,
    ) -> Option<Self> {
        assert!(
            imported.is_none_or(|imported| imported.mir.panic_strategy == strategy),
            "dependency compatibility must be checked before importing MIR"
        );
        if let Some(library) = crate::library::LibraryMir::of_with_strategy(hir, strategy)
            .filter(|library| library.owns(owner))
        {
            Some(Self::Library(library))
        } else {
            imported
                .filter(|imported| imported.owns(owner, hir.items.core))
                .map(Self::Imported)
        }
    }

    pub(crate) fn built(
        &self,
        items: &ItemTree,
        sigs: &flux_ty::Signatures,
        owner: ClosureOwner,
    ) -> Option<crate::build::Built> {
        match self {
            Self::Library(library) => match owner {
                ClosureOwner::Function(id) => library.function(id).cloned(),
                ClosureOwner::Const(id) => Some(library.konst(id).clone()),
            },
            Self::Imported(imported) => imported.built(items, sigs, owner),
        }
    }
}
