//! Region substitution for a concrete associated type selected at its written occurrence.
use super::*;

/// Normalize only occurrences whose written qualified Self can supply a row.
/// Family projections retain their existing captured-input representation.
pub(super) fn field_type(items: &ItemTree, sigs: &Signatures, written: &TypeRef, ty: Ty) -> Ty {
    match (written, ty.kind()) {
        (TypeRef::Path(path), TyKind::Alias(alias))
            if path.qself.is_some() && !has_declared_projection_inputs(alias) =>
        {
            let output = flux_solve::traits::normalize(items, sigs, &[], ty);
            if output.has_aliases() { ty } else { output }
        }
        (TypeRef::Ptr { pointee, .. }, TyKind::RawPtr(..))
        | (TypeRef::Array { elem: pointee, .. }, TyKind::Array(..))
        | (TypeRef::Slice(pointee), TyKind::Slice(_)) => {
            ty.map_children(|child| field_type(items, sigs, pointee, child))
        }
        (TypeRef::Tuple(elements), TyKind::Tuple(_)) => {
            let mut elements = elements.iter();
            ty.map_children(|child| {
                elements
                    .next()
                    .map_or(child, |written| field_type(items, sigs, written, child))
            })
        }
        (TypeRef::Path(path), TyKind::Adt(..)) => {
            let mut arguments = path
                .args(path.segments.len().saturating_sub(1))
                .into_iter()
                .flat_map(|args| args.types.iter());
            ty.map_children(|child| {
                arguments
                    .next()
                    .map_or(child, |written| field_type(items, sigs, written, child))
            })
        }
        // An outer reference adds an output-outlives obligation on the
        // projection itself. Keep that original subject until declaration
        // checking can prove it, rather than infer it from the normalized RHS.
        _ => ty,
    }
}

/// Fully concrete, region-free occurrences need no syntax-to-output row.
/// Their owner and family requirements still need independent type proofs;
/// lifetime-dependent selected headers retain the ordinary occurrence path.
pub(super) fn closed_type_projection(
    items: &ItemTree,
    sigs: &Signatures,
    alias: crate::AliasTy,
    output: Ty,
) -> bool {
    let closed = |ty: Ty| !ty.has_infer() && is_closed_region_free(items, ty);
    if alias.owner_lifetimes != 0
        || !alias
            .args
            .iter()
            .chain(alias.family_args)
            .copied()
            .all(closed)
        || !closed(output)
    {
        return false;
    }
    let proof = flux_solve::traits::RigidSolver::new(items, sigs, &[], &[], &[]);
    let Some((imp, impl_args)) = proof.select_impl(flux_solve::traits::alias_predicate(alias))
    else {
        return false;
    };
    let Some(metadata) = sigs.regions.impls.get(imp) else {
        return false;
    };
    let row_free = |rows: &TraitRefRegions| {
        rows.bound == 0 && rows.lifetimes.is_empty() && rows.args.iter().all(|row| row.is_empty())
    };
    if !metadata.names.is_empty()
        || !metadata.bounds.is_empty()
        || metadata.bound_selfs.iter().any(|row| !row.is_empty())
        || !metadata.bound_traits.iter().all(row_free)
    {
        return false;
    }
    let declaration = sigs
        .regions
        .traits
        .get(alias.trait_)
        .and_then(|regions| {
            regions
                .families
                .iter()
                .find(|(name, _)| *name == alias.name)
        })
        .map(|(_, family)| family);
    let implementation = metadata
        .families
        .iter()
        .find(|(name, _)| *name == alias.name)
        .map(|(_, family)| family);
    if alias.family_lifetimes != 0 && (declaration.is_none() || implementation.is_none()) {
        return false;
    }
    let mut selected_args = impl_args;
    selected_args.extend_from_slice(alias.family_args);
    let declaration_args = alias.template_args();
    for (family, args) in declaration
        .map(|family| (family, declaration_args.as_slice()))
        .into_iter()
        .chain(implementation.map(|family| (family, selected_args.as_slice())))
    {
        // A closed type is static, so Self:'a and T:'a hold for every exact
        // family lifetime occurrence. Nontrivial lifetime relations and
        // lifetime-dependent membership still need their original row proof.
        if family.owner != 0
            || family.names.len() != alias.family_lifetimes as usize
            || !family.value.is_empty()
            || family.bound_selfs.iter().any(|row| !row.is_empty())
            || !family.bound_traits.iter().all(row_free)
            || family.projections.iter().any(|row| !row.is_empty())
            || family.selections.iter().any(|selection| {
                !selection.lifetimes.is_empty()
                    || selection.args.iter().any(|row| !row.is_empty())
                    || selection.alias.owner_lifetimes != 0
                    || selection.alias.family_lifetimes != 0
            })
            || !family.outlives.iter().all(|bound| match *bound {
                Outlives::Region(a, b) => a == b || a == Region::Static,
                Outlives::Type(ty, _) => closed(ty.subst(args)),
            })
        {
            return false;
        }
    }
    let requirements = flux_solve::traits::family_requirements(items, sigs, alias);
    requirements
        .predicates
        .iter()
        .all(|&predicate| proof.holds(predicate, &mut Vec::new()))
        && requirements.projections.iter().all(|&projection| {
            let lhs = proof.normalize_ty(Ty::alias(projection.alias), None);
            let rhs = proof.normalize_rhs(projection);
            !lhs.has_error() && !rhs.has_error() && lhs == rhs
        })
        && proof.normalize_ty(Ty::alias(alias), None) == output
}

/// A generic projection may change type without changing a represented row.
/// Prove every selected edge independently; erased equality alone is not proof
/// that a selected impl's lifetime requirements hold.
pub(super) fn generic_zero_row_projection(
    items: &ItemTree,
    sigs: &Signatures,
    alias: crate::AliasTy,
    output: Ty,
    scope: TypeScope<'_>,
    caller: &[crate::Projection],
) -> bool {
    let proof = flux_solve::traits::RigidSolver::new(
        items,
        sigs,
        scope.bounds,
        caller,
        scope.unsized_params,
    )
    .with_abstract_outputs(scope.bounds);
    let mut check = ZeroRowSelection {
        items,
        sigs,
        env: scope.bounds,
        proof,
        aliases: Vec::new(),
        goals: Vec::new(),
    };
    check.ty(output, 0)
        && check.projection(alias, 0)
        && check.proof.normalize_ty(Ty::alias(alias), None) == output
}

struct ZeroRowSelection<'a> {
    items: &'a ItemTree,
    sigs: &'a Signatures,
    env: &'a [crate::Predicate],
    proof: flux_solve::traits::RigidSolver<'a>,
    aliases: Vec<crate::AliasTy>,
    goals: Vec<crate::Predicate>,
}

impl ZeroRowSelection<'_> {
    fn ty(&mut self, ty: Ty, depth: u32) -> bool {
        if depth > 32
            || ty.has_error()
            || ty.has_infer()
            || ty.has_opaques()
            || slots(self.items, ty) != 0
        {
            return false;
        }
        if let TyKind::Alias(alias) = ty.kind() {
            return self.projection(alias, depth + 1);
        }
        ty.children().all(|child| self.ty(child, depth + 1))
    }

    fn goal(&mut self, goal: crate::Predicate, depth: u32) -> bool {
        if depth > 32
            || self.goals.contains(&goal)
            || !self.ty(goal.self_ty, depth + 1)
            || !goal.trait_ref.args.iter().all(|&ty| self.ty(ty, depth + 1))
        {
            return false;
        }
        // Caller memberships and their declared output bounds own these exact
        // abstract types. Do not replace them with a possible concrete impl.
        if flux_solve::traits::in_env(self.sigs, self.env, goal) {
            return true;
        }
        if flux_solve::traits::is_builtin_trait(self.items, goal.trait_ref.trait_, goal.self_ty) {
            return true;
        }
        let Some((imp, args)) = self.proof.select_impl(goal) else {
            return false;
        };
        self.goals.push(goal);
        let result = self.contract(imp, &args, depth + 1);
        self.goals.pop();
        result
    }

    fn contract(&mut self, imp: flux_hir::ImplId, args: &[Ty], depth: u32) -> bool {
        let Some(rows) = self.sigs.regions.impls.get(imp) else {
            return false;
        };
        let predicates = self.sigs.impl_predicates.get(imp).map_or(&[][..], |p| p);
        if depth > 32
            || !rows.names.is_empty()
            || !rows.bounds.is_empty()
            || !rows.trait_lifetimes.is_empty()
            || !rows.self_ty.is_empty()
            || rows.trait_args.iter().any(|row| !row.is_empty())
            || rows.bound_selfs.iter().any(|row| !row.is_empty())
            || rows.bound_traits.iter().any(|row| !Self::row_free(row))
            || rows.header_selections.iter().any(|row| !row.is_empty())
            || self
                .sigs
                .impl_projections
                .get(imp)
                .is_some_and(|rows| !rows.is_empty())
            || self
                .sigs
                .impl_traits
                .get(imp)
                .and_then(|tr| self.sigs.regions.traits.get(tr.trait_))
                .is_some_and(|rows| !rows.names.is_empty() || !rows.outlives.is_empty())
        {
            return false;
        }
        predicates
            .iter()
            .all(|p| self.goal(p.subst(args), depth + 1))
    }

    fn row_free(row: &TraitRefRegions) -> bool {
        row.bound == 0 && row.lifetimes.is_empty() && row.args.iter().all(|row| row.is_empty())
    }

    fn projection(&mut self, alias: crate::AliasTy, depth: u32) -> bool {
        if depth > 32
            || has_declared_projection_inputs(alias)
            || self.aliases.contains(&alias)
            || !alias.args.iter().all(|&ty| self.ty(ty, depth + 1))
        {
            return false;
        }
        let goal = flux_solve::traits::alias_predicate(alias);
        if !self.goal(goal, depth + 1) {
            return false;
        }
        self.aliases.push(alias);
        let result = if flux_solve::traits::in_env(self.sigs, self.env, goal) {
            let normalized = self.proof.normalize_ty(Ty::alias(alias), None);
            normalized == Ty::alias(alias) || self.ty(normalized, depth + 1)
        } else if let Some((imp, args)) = self.proof.select_impl(goal) {
            let Some(rows) = self.sigs.regions.impls.get(imp) else {
                self.aliases.pop();
                return false;
            };
            let family_ok = rows
                .families
                .iter()
                .find(|(name, _)| *name == alias.name)
                .is_none_or(|(_, family)| {
                    family.names.is_empty()
                        && family.outlives.is_empty()
                        && family.output_outlives.is_empty()
                        && family.value.is_empty()
                        && family.bound_selfs.iter().all(|row| row.is_empty())
                        && family.bound_traits.iter().all(Self::row_free)
                        && family
                            .output_traits
                            .iter()
                            .all(|(_, row)| Self::row_free(row))
                        && family.projections.iter().all(|row| row.is_empty())
                        && family.selections.iter().all(|selection| {
                            !has_declared_projection_inputs(selection.alias)
                                && selection.lifetimes.is_empty()
                                && selection.args.iter().all(|row| row.is_empty())
                        })
                });
            let selections_ok = rows
                .type_selections
                .iter()
                .filter(|(name, _)| *name == alias.name)
                .flat_map(|(_, selections)| selections.iter())
                .all(|selection| {
                    !has_declared_projection_inputs(selection.alias)
                        && selection.lifetimes.is_empty()
                        && selection.args.iter().all(|row| row.is_empty())
                });
            family_ok
                && selections_ok
                && flux_solve::traits::impl_type(self.sigs, imp, alias.name)
                    .is_some_and(|ty| self.ty(ty.subst(&args), depth + 1))
        } else {
            false
        };
        self.aliases.pop();
        result
    }
}

pub(crate) fn projection(
    items: &ItemTree,
    sigs: &Signatures,
    alias: crate::AliasTy,
    inputs: &[Region],
) -> Option<Vec<Region>> {
    projection_inner(items, sigs, alias, inputs, false)
}

/// A closed output still requires a definite membership with matching owner
/// lifetimes. Its zero output slots do not prove the impl header.
pub(super) fn closed_projection(
    items: &ItemTree,
    sigs: &Signatures,
    alias: crate::AliasTy,
    inputs: &[Region],
) -> Option<Vec<Region>> {
    let output = flux_solve::traits::normalize(items, sigs, &[], Ty::alias(alias));
    is_closed_region_free(items, output).then_some(())?;
    projection_inner(items, sigs, alias, inputs, true)
}

fn projection_inner(
    items: &ItemTree,
    sigs: &Signatures,
    alias: crate::AliasTy,
    inputs: &[Region],
    allow_owner: bool,
) -> Option<Vec<Region>> {
    let goal = crate::Predicate {
        self_ty: *alias.args.first()?,
        trait_ref: crate::TraitRef {
            trait_: alias.trait_,
            name: alias.trait_name,
            args: &alias.args[1..],
        },
    };
    let (imp, args) = if allow_owner {
        flux_solve::traits::select_definite_impl(items, sigs, goal)?
    } else {
        flux_solve::traits::select_impl(items, sigs, goal)?
    };
    let metadata = sigs.regions.impls.get(imp)?;
    if (!allow_owner && !metadata.trait_lifetimes.is_empty())
        || !metadata.bounds.is_empty()
        || (has_declared_projection_inputs(alias)
            && (!allow_owner || alias.family_lifetimes != 0 || !alias.family_args.is_empty()))
        || (allow_owner && (metadata.header_selections.iter().any(|rows| !rows.is_empty())
            || metadata.type_selections.iter().any(|(name, rows)| *name == alias.name && !rows.is_empty())))
        // A nested projection's zero-slot legacy type is not its input row.
        // It needs an independent occurrence mapping before header binding.
        || alias.args.iter().any(|ty| ty.has_aliases())
        || sigs
            .regions
            .traits
            .get(alias.trait_)
            .is_some_and(|r| !r.outlives.is_empty())
        || sigs.impl_predicates.get(imp).is_some_and(|predicates| {
            predicates.iter().any(|predicate| {
                let predicate = predicate.subst(&args);
                slots(items, predicate.self_ty) != 0
                    || predicate
                        .trait_ref
                        .args
                        .iter()
                        .any(|&ty| slots(items, ty) != 0)
                    || !items.traits[predicate.trait_ref.trait_]
                        .generics
                        .lifetimes
                        .is_empty()
            })
        })
    {
        return None;
    }
    let mut binding = Binding {
        items,
        args: &args,
        lifetimes: vec![None; metadata.names.len()],
        types: vec![None; args.len()],
    };
    let trait_args = sigs.impl_traits.get(imp)?.args;
    if metadata.trait_args.len() != trait_args.len() || alias.args.len() != trait_args.len() + 1 {
        return None;
    }
    let patterns = std::iter::once(sigs.impls[imp]).chain(trait_args.iter().copied());
    let tables = std::iter::once(&metadata.self_ty).chain(metadata.trait_args.iter());
    let mut at = 0;
    if allow_owner {
        if metadata.trait_lifetimes.len() != alias.owner_lifetimes as usize {
            return None;
        }
        for (&formal, &actual) in metadata
            .trait_lifetimes
            .iter()
            .zip(inputs.get(..alias.owner_lifetimes as usize)?)
        {
            binding.bind_lifetime(formal, actual)?;
        }
        at = alias.owner_lifetimes as usize;
    }
    for ((pattern, table), &actual) in patterns.zip(tables).zip(alias.args) {
        let count = slots(items, actual);
        binding.bind(pattern, table, actual, inputs.get(at..at + count)?)?;
        at += count;
    }
    if at != inputs.len() {
        return None;
    }
    if args.iter().zip(&binding.types).any(|(&ty, row)| {
        row.as_ref()
            .map_or(slots(items, ty) != 0, |row| row.len() != slots(items, ty))
    }) {
        return None;
    }
    let template = flux_solve::traits::impl_type(sigs, imp, alias.name)?;
    // Unresolved child projections keep their original identity and slots.
    // A further selection that changes this RHS shape needs its own occurrence
    // mapping; never infer that row from a recursively normalized output.
    let output = template.subst(&args);
    if output != flux_solve::traits::normalize(items, sigs, &[], Ty::alias(alias)) {
        return None;
    }
    let own = &metadata
        .types
        .iter()
        .find(|(name, _)| *name == alias.name)?
        .1;
    let mut complete = true;
    let mut map = |region| match region {
        Region::Static => Region::Static,
        Region::Param(i) => binding
            .lifetimes
            .get(i as usize)
            .copied()
            .flatten()
            .unwrap_or_else(|| {
                complete = false;
                Region::Erased
            }),
        _ => {
            complete = false;
            Region::Erased
        }
    };
    let types = |index: u32| {
        binding
            .types
            .get(index as usize)
            .cloned()
            .flatten()
            .unwrap_or_else(|| vec![Region::Erased; slots(items, args[index as usize])])
    };
    let mut result = Vec::new();
    instantiate(items, template, own, &mut map, &types, &mut result);
    (complete && result.len() == slots(items, output)).then_some(result)
}

struct Binding<'a> {
    items: &'a ItemTree,
    args: &'a [Ty],
    lifetimes: Vec<Option<Region>>,
    types: Vec<Option<Vec<Region>>>,
}

impl Binding<'_> {
    fn bind_lifetime(&mut self, formal: Region, actual: Region) -> Option<()> {
        match formal {
            Region::Param(i) => {
                let slot = self.lifetimes.get_mut(i as usize)?;
                if slot.is_some_and(|previous| previous != actual) {
                    return None;
                }
                *slot = Some(actual);
                Some(())
            }
            Region::Static if actual == Region::Static => Some(()),
            _ => None,
        }
    }

    fn bind(&mut self, pattern: Ty, own: &[Region], actual: Ty, row: &[Region]) -> Option<()> {
        if pattern.subst(self.args) != actual || row.len() != slots(self.items, actual) {
            return None;
        }
        if let TyKind::Param(param) = pattern.kind() {
            let slot = self.types.get_mut(param.index as usize)?;
            if slot.as_ref().is_some_and(|previous| previous != row) {
                return None;
            }
            *slot = Some(row.to_vec());
            return Some(());
        }
        let direct = match pattern.kind() {
            TyKind::Ref(..) => 1,
            TyKind::Dynamic(object) => super::object_own_slots(self.items, object),
            TyKind::Adt(adt, _) => self.items.structs[adt.strukt].generics.lifetimes.len(),
            TyKind::Alias(_) | TyKind::Opaque(_) | TyKind::Closure(..) | TyKind::FnPtr(_) => {
                return (own.is_empty() && row.is_empty()).then_some(());
            }
            _ => 0,
        };
        for (&formal, &value) in own.get(..direct)?.iter().zip(row.get(..direct)?) {
            match formal {
                Region::Param(i) => {
                    let slot = self.lifetimes.get_mut(i as usize)?;
                    if slot.is_some_and(|previous| previous != value) {
                        return None;
                    }
                    *slot = Some(value);
                }
                Region::Static if value == Region::Static => {}
                _ => return None,
            }
        }
        let (mut p, mut a) = (direct, direct);
        for (pattern, actual) in pattern.children().zip(actual.children()) {
            let pn = slots(self.items, pattern);
            let an = slots(self.items, actual);
            self.bind(pattern, own.get(p..p + pn)?, actual, row.get(a..a + an)?)?;
            p += pn;
            a += an;
        }
        (p == own.len() && a == row.len()).then_some(())
    }
}
