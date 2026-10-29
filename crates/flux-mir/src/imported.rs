//! Checked dependency bodies supplied without dependency HIR or inference.

use flux_hir::flib::Error;
use flux_hir::{ConstId, FunctionId, ItemCounts};
use flux_ty::{ClosureId, ClosureKind, ClosureOwner};

use crate::{Body, FileMir};

/// A checked MIR snapshot loaded before the source crate is collected.
#[derive(Debug)]
pub struct ImportedMir {
    mir: FileMir,
    source_counts: ItemCounts,
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
            || mir.closures.keys().any(|(id, _)| match id.owner {
                ClosureOwner::Function(f) => f.into_raw() >= source_counts.functions,
                ClosureOwner::Const(c) => c.into_raw() >= source_counts.consts,
            })
        {
            return Err(Error::new("imported MIR exceeds source boundary"));
        }
        Ok(Self { mir, source_counts })
    }

    /// The boundary captured before adding the source crate.
    pub fn source_counts(&self) -> ItemCounts {
        self.source_counts
    }

    /// A dependency function's already checked generic template.
    pub fn function(&self, id: FunctionId) -> Option<&Body> {
        self.mir.bodies.get(id)
    }

    /// A dependency constant's checked initializer.
    pub fn konst(&self, id: ConstId) -> Option<&Body> {
        self.mir.consts.get(id)
    }

    /// Checked closure bodies and callable shims belonging to one dependency owner.
    pub fn closures(
        &self,
        owner: ClosureOwner,
    ) -> impl Iterator<Item = ((ClosureId, ClosureKind), &Body)> {
        self.mir
            .closures
            .iter()
            .filter(move |((id, _), _)| id.owner == owner)
            .map(|(&key, body)| (key, body))
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
    ) -> Vec<FunctionId> {
        let mut roots = std::collections::BTreeSet::new();
        let mut values = Vec::new();
        for body in self
            .mir
            .bodies
            .iter()
            .map(|(_, body)| body)
            .chain(self.mir.consts.iter().map(|(_, body)| body))
            .chain(self.mir.closures.values())
        {
            for function in body.callees().into_iter().chain(body.object_methods(items)) {
                roots.extend(flux_ty::call_targets(items, function));
            }
            values.extend(body.promoted.iter());
        }
        values.extend(self.mir.const_values.items.iter().map(|(_, value)| value));
        values.extend(self.mir.const_values.instances.values());
        for value in values {
            let mut instances = Vec::new();
            crate::mono::value_functions(
                value,
                &self.mir.const_values,
                items,
                sigs,
                &mut instances,
            );
            for instance in instances {
                roots.extend(flux_ty::call_targets(items, instance.function));
            }
        }
        roots.into_iter().collect()
    }

    /// Copies checked dependency MIR into a fresh compilation's evaluation tables.
    /// Existing entries are errors rather than silently replacing checked bodies.
    pub fn seed_into(&self, target: &mut FileMir) -> Result<(), Error> {
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
                .any(|(id, _)| target.const_values.items.get(id).is_some())
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
        match owner {
            ClosureOwner::Function(id) => {
                (core.functions..self.source_counts.functions).contains(&id.into_raw())
            }
            ClosureOwner::Const(id) => {
                (core.consts..self.source_counts.consts).contains(&id.into_raw())
            }
        }
    }

    pub(crate) fn built(&self, owner: ClosureOwner) -> Option<crate::build::Built> {
        let body = match owner {
            ClosureOwner::Function(id) => self.function(id),
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
        if let Some(library) =
            crate::library::LibraryMir::of(hir).filter(|library| library.owns(owner))
        {
            Some(Self::Library(library))
        } else {
            imported
                .filter(|imported| imported.owns(owner, hir.items.core))
                .map(Self::Imported)
        }
    }

    pub(crate) fn built(&self, owner: ClosureOwner) -> Option<crate::build::Built> {
        match self {
            Self::Library(library) => match owner {
                ClosureOwner::Function(id) => library.function(id).cloned(),
                ClosureOwner::Const(id) => Some(library.konst(id).clone()),
            },
            Self::Imported(imported) => imported.built(owner),
        }
    }
}
