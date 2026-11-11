//! Associated family occurrence inputs and selected impl template substitution.
use super::*;

struct FamilyInputs {
    alias: AliasTy,
    owner: Vec<Var>,
    inputs: Vec<Vec<Var>>,
    family: Vec<Var>,
    family_inputs: Vec<Vec<Var>>,
}

impl Cx<'_> {
    /// An abstract output inherits bounds from its independently proven owner.
    /// Its row captures the owning trait occurrence, not the output's fields.
    pub(super) fn associated_output_bounds(
        &mut self,
        goal: &ImplRegionGoal,
        env: &[ImplRegionGoal],
        depth: u32,
    ) -> Option<Vec<ImplRegionGoal>> {
        let TyKind::Alias(alias) = goal.predicate.self_ty.kind() else {
            return None;
        };
        let row = goal.vars.first()?;
        self.proven_family_output(alias, row, env, depth)
            .map(|(_, bounds)| bounds)
    }

    /// Return a declaration's output guarantees only after proving this exact
    /// projection occurrence's owner and family prerequisites.
    pub(super) fn proven_family_output(
        &mut self,
        alias: AliasTy,
        row: &[Var],
        env: &[ImplRegionGoal],
        depth: u32,
    ) -> Option<(Vec<Var>, Vec<ImplRegionGoal>)> {
        if !flux_ty::regions::has_projection_inputs(self.items, alias) {
            return None;
        }
        let owner = row.get(..alias.owner_lifetimes as usize)?;
        let mut next = owner.len();
        let mut inputs = Vec::new();
        for &ty in alias.args {
            let end = next.checked_add(self.slots(ty) as usize)?;
            inputs.push(row.get(next..end)?.to_vec());
            next = end;
        }
        let family_end = next.checked_add(alias.family_lifetimes as usize)?;
        let family = row.get(next..family_end)?;
        next = family_end;
        let mut family_inputs = Vec::new();
        for &ty in alias.family_args {
            let end = next.checked_add(self.slots(ty) as usize)?;
            family_inputs.push(row.get(next..end)?.to_vec());
            next = end;
        }
        if next != row.len() {
            return None;
        }
        let owner_goal = ImplRegionGoal {
            predicate: flux_ty::Predicate {
                self_ty: *alias.args.first()?,
                trait_ref: flux_ty::TraitRef {
                    trait_: alias.trait_,
                    name: alias.trait_name,
                    args: &alias.args[1..],
                },
            },
            lifetimes: owner.to_vec(),
            vars: inputs.clone(),
            bound: Vec::new(),
        };
        if !self.impl_prove_super(&owner_goal, env, depth + 1) {
            return None;
        }
        self.family_prerequisites(alias, owner, &inputs, family, &family_inputs)?;
        let lifetimes: Vec<_> = owner.iter().chain(family).copied().collect();
        let types: Vec<_> = inputs.into_iter().chain(family_inputs).collect();
        let schema = self
            .sigs
            .traits
            .get(alias.trait_)?
            .families
            .iter()
            .find(|(name, _)| *name == alias.name)?
            .1
            .clone();
        // Family-local equalities need their own output region relation.
        // Do not assume them while proving the projection's prerequisites.
        if !schema.projections.is_empty() {
            return None;
        }
        let contract = &self.sigs.regions.traits[alias.trait_]
            .families
            .iter()
            .find(|(name, _)| *name == alias.name)?
            .1;
        let args = alias.template_args();
        for (index, &predicate) in schema.predicates.iter().enumerate() {
            let requirement = self.impl_region_goal_with_proof(
                predicate,
                contract.bound_selfs.get(index).map_or(&[], |row| row),
                contract
                    .bound_traits
                    .get(index)
                    .unwrap_or(&TraitRefRegions::default()),
                &lifetimes,
                &args,
                &types,
                Some((env, depth + 1)),
                false,
            )?;
            if !self.impl_prove_super(&requirement, env, depth + 1) {
                return None;
            }
        }
        let bounds = self.declared_output_traits(alias, row, &lifetimes, &types, depth)?;
        Some((lifetimes, bounds))
    }

    /// Declared bounds belong to a proven projection occurrence, including its
    /// owner and family prerequisites. They are not facts about an erased Ty.
    fn family_outlives_sources(
        &mut self,
        value: Ty,
        row: &[Var],
        env: &[ImplRegionGoal],
    ) -> Option<Vec<Var>> {
        let TyKind::Alias(alias) = value.kind() else {
            return None;
        };
        let (lifetimes, _) = self.proven_family_output(alias, row, env, 0)?;
        let contract = &self
            .sigs
            .regions
            .traits
            .get(alias.trait_)?
            .families
            .iter()
            .find(|(name, _)| *name == alias.name)?
            .1;
        Some(
            contract
                .output_outlives
                .iter()
                .filter_map(|bound| match *bound {
                    Region::Static => Some(STATIC),
                    Region::Param(index) => lifetimes.get(index as usize).copied(),
                    Region::Erased | Region::Late(_) => None,
                })
                .collect(),
        )
    }

    /// Constrain an inferred call lifetime from an exact associated output row.
    /// Multiple independent bounds need a disjunctive proof; keep that case on
    /// the existing conservative path rather than choose an arbitrary bound.
    pub(super) fn constrain_family_outlives(
        &mut self,
        value: Ty,
        row: &[Var],
        target: Var,
        env: &[ImplRegionGoal],
        range: TextRange,
    ) -> bool {
        let Some(mut sources) = self.family_outlives_sources(value, row, env) else {
            return false;
        };
        if sources.contains(&STATIC) {
            return true;
        }
        sources.sort_unstable();
        sources.dedup();
        let [source] = sources.as_slice() else {
            return false;
        };
        self.outlives(*source, target, Category::CallArgument, range);
        true
    }

    /// A declaration proof can use an output lifetime bound from this exact
    /// family occurrence. Captured owner inputs are not the output's fields,
    /// and an erased alias cannot supply a global type-outlives fact.
    fn family_output_outlives(
        &mut self,
        value: Ty,
        row: &[Var],
        target: Var,
        env: &[ImplRegionGoal],
    ) -> bool {
        let TyKind::Alias(alias) = value.kind() else {
            return false;
        };
        let Some(sources) = self.family_outlives_sources(value, row, env) else {
            return false;
        };
        let declared = sources.into_iter().any(|source| {
            source == STATIC
                || source == target
                || match (
                    self.universal_of.get(&source),
                    self.universal_of.get(&target),
                ) {
                    (Some(&source), Some(&target)) => self.known(source, target),
                    _ => false,
                }
        });
        if declared {
            return true;
        }
        // A well-formed projection outlives a region when every input does.
        // These are occurrence inputs, not the unknown output's fields. Keep
        // both their concrete regions and their abstract type requirements.
        for &region in row {
            self.outlives(region, target, Category::Boring, self.call_range);
        }
        for &argument in alias.template_args().iter() {
            self.type_outlives(argument, target, self.call_range);
        }
        true
    }

    /// Instantiate declaration bounds with the exact captured projection inputs.
    fn declared_output_traits(
        &mut self,
        alias: AliasTy,
        row: &[Var],
        lifetimes: &[Var],
        inputs: &[Vec<Var>],
        depth: u32,
    ) -> Option<Vec<ImplRegionGoal>> {
        let contract = self
            .sigs
            .regions
            .traits
            .get(alias.trait_)?
            .families
            .iter()
            .find(|(name, _)| *name == alias.name)?
            .1
            .clone();
        if contract.owner != alias.owner_lifetimes
            || contract.names.len() != lifetimes.len()
            || lifetimes.len() != (alias.owner_lifetimes + alias.family_lifetimes) as usize
            || inputs.len() != alias.args.len() + alias.family_args.len()
        {
            return None;
        }
        let args = alias.template_args();
        // Bound arguments use the owner's parameters. Attach the separately
        // represented output row after substituting those arguments.
        let mut bounds = Vec::new();
        for (bound, table) in &contract.output_traits {
            let mut bound = self.impl_region_goal_with_proof(
                bound.with_self(Ty::param(0, flux_intern::kw::SelfUpper)),
                &[],
                table,
                lifetimes,
                &args,
                inputs,
                None,
                true,
            )?;
            bound.predicate.self_ty = Ty::alias(alias);
            bound.vars[0] = row.to_vec();
            if !self.impl_super_goals(bound, &mut bounds, depth + 1) {
                return None;
            }
        }
        Some(bounds)
    }

    /// Instantiate a projection's captured input row before normalizing its
    /// output. Owner and family binders have independent variable arrays.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn instantiate_family<A: AsRef<[Var]>>(
        &mut self,
        alias: AliasTy,
        own: &[Region],
        next: &mut usize,
        map: &mut dyn FnMut(Region) -> Option<Var>,
        args: &[Ty],
        arg_vars: &[A],
    ) -> Option<Vec<Var>> {
        let FamilyInputs {
            alias: instance,
            owner,
            inputs,
            family,
            family_inputs,
        } = self.family_inputs(alias, own, next, map, args, arg_vars)?;
        self.family_prerequisites(instance, &owner, &inputs, &family, &family_inputs)?;
        if self.family_depth > 32 {
            return None;
        }
        self.family_depth += 1;
        let output = self.family_output(
            instance,
            &owner,
            &inputs,
            &family,
            &family_inputs,
            self.family_depth,
        );
        self.family_depth -= 1;
        output
    }

    /// Preserve the owning inputs even when selecting the associated output
    /// changes its region layout. Output fields cannot prove owner obligations.
    #[allow(clippy::too_many_arguments)]
    fn family_inputs<A: AsRef<[Var]>>(
        &mut self,
        alias: AliasTy,
        own: &[Region],
        next: &mut usize,
        map: &mut dyn FnMut(Region) -> Option<Var>,
        args: &[Ty],
        arg_vars: &[A],
    ) -> Option<FamilyInputs> {
        let mut owner = Vec::new();
        for _ in 0..alias.owner_lifetimes {
            owner.push(map(*own.get(*next)?)?);
            *next += 1;
        }
        let mut inputs = Vec::new();
        for &ty in alias.args {
            let mut row = Vec::new();
            if !self.inst(ty, own, next, map, args, arg_vars, &mut row) {
                return None;
            }
            inputs.push(row);
        }
        let mut family = Vec::new();
        for _ in 0..alias.family_lifetimes {
            family.push(map(*own.get(*next)?)?);
            *next += 1;
        }
        let mut family_inputs = Vec::new();
        for &ty in alias.family_args {
            let mut row = Vec::new();
            if !self.inst(ty, own, next, map, args, arg_vars, &mut row) {
                return None;
            }
            family_inputs.push(row);
        }
        let instance = if args.is_empty() {
            alias
        } else {
            let TyKind::Alias(instance) = Ty::alias(alias).subst(args).kind() else {
                return None;
            };
            instance
        };
        Some(FamilyInputs {
            alias: instance,
            owner,
            inputs,
            family,
            family_inputs,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn written_family_alias_proven(
        &mut self,
        alias: AliasTy,
        own: &[Region],
        map: &mut dyn FnMut(Region) -> Option<Var>,
        args: &[Ty],
        arg_vars: &[Vec<Var>],
        env: &[ImplRegionGoal],
    ) -> bool {
        let mut next = 0;
        let Some(inputs) = self.family_inputs(alias, own, &mut next, map, args, arg_vars) else {
            return false;
        };
        if next != own.len() {
            return false;
        }
        let row = inputs
            .owner
            .iter()
            .chain(inputs.inputs.iter().flatten())
            .chain(inputs.family.iter())
            .chain(inputs.family_inputs.iter().flatten())
            .copied()
            .collect::<Vec<_>>();
        self.proven_family_output(inputs.alias, &row, env, 0)
            .is_some()
    }

    #[allow(clippy::too_many_arguments)]
    fn family_prerequisites(
        &mut self,
        alias: AliasTy,
        owner: &[Var],
        inputs: &[Vec<Var>],
        family: &[Var],
        family_inputs: &[Vec<Var>],
    ) -> Option<()> {
        let decl = self
            .sigs
            .regions
            .traits
            .get(alias.trait_)?
            .families
            .iter()
            .find(|(name, _)| *name == alias.name)?
            .1
            .clone();
        if decl.owner as usize != owner.len() || decl.names.len() != owner.len() + family.len() {
            return None;
        }
        let lifetimes: Vec<_> = owner.iter().chain(family).copied().collect();
        let types: Vec<_> = inputs.iter().chain(family_inputs).cloned().collect();
        let args = alias.template_args();
        let map = |r| match r {
            Region::Static => Some(STATIC),
            Region::Param(i) => lifetimes.get(i as usize).copied(),
            _ => None,
        };
        for prerequisite in &decl.outlives {
            match *prerequisite {
                Outlives::Region(a, b) => {
                    self.outlives(map(a)?, map(b)?, Category::CallArgument, self.call_range)
                }
                Outlives::Type(ty, b) => {
                    let b = map(b)?;
                    let vars = self.bound_slots(ty, &args, &types)?;
                    for v in vars {
                        self.outlives(v, b, Category::CallArgument, self.call_range);
                    }
                    self.type_outlives(self.normalize(ty.subst(&args)), b, self.call_range);
                }
            }
        }
        Some(())
    }

    /// An ordinary associated equality has no family-local arguments, but
    /// trait lifetime parameters still belong to the original bound occurrence.
    pub(super) fn ordinary_projection_output(
        &mut self,
        alias: AliasTy,
        owner: &[Var],
        inputs: &[Vec<Var>],
    ) -> Option<Vec<Var>> {
        if alias.owner_lifetimes as usize != owner.len()
            || alias.family_lifetimes != 0
            || !alias.family_args.is_empty()
        {
            return None;
        }
        if let TyKind::Opaque(opaque) = alias.args[0].kind()
            && self
                .sigs
                .opaques
                .get(opaque.function)
                .and_then(|sigs| sigs.get(opaque.index as usize))
                .is_some_and(|sig| {
                    sig.projections.iter().any(|projection| {
                        projection
                            .subst(&flux_solve::traits::opaque_args(opaque))
                            .alias
                            == alias
                    })
                })
        {
            return self.opaque_equality_output(opaque, alias, owner, inputs);
        }
        let normalized = alias.map_args(|ty| self.normalize(ty));
        if normalized != alias {
            if normalized.args.len() != inputs.len()
                || normalized
                    .args
                    .iter()
                    .zip(inputs)
                    .any(|(&ty, row)| self.slots(ty) as usize != row.len())
            {
                return None;
            }
            // Input occurrences were instantiated before selection. Their
            // types must reflect any associated outputs proved along the way.
            return self.ordinary_projection_output(normalized, owner, inputs);
        }
        if owner.is_empty() && !flux_ty::regions::has_projection_inputs(self.items, alias) {
            self.normalize_regions(alias, inputs, 0)
        } else {
            self.family_prerequisites(alias, owner, inputs, &[], &[])?;
            self.family_output(alias, owner, inputs, &[], &[], 0)
        }
    }

    /// The selected RHS is expressed in selected header parameters followed
    /// by family parameters, never the trait's erased membership arguments.
    #[allow(clippy::too_many_arguments)]
    fn family_output(
        &mut self,
        alias: AliasTy,
        owner: &[Var],
        inputs: &[Vec<Var>],
        family: &[Var],
        family_inputs: &[Vec<Var>],
        depth: u32,
    ) -> Option<Vec<Var>> {
        if let Some(scheme) = self.env_ranked.iter().find(|s| s.alias == alias).cloned() {
            return self.ranked_projection_output(&scheme, owner, inputs);
        }
        if owner.is_empty()
            && family.is_empty()
            && family_inputs.is_empty()
            && matches!(
                alias.args.first().map(|t| t.kind()),
                Some(TyKind::Dynamic(_))
            )
        {
            return self.normalize_regions(alias, inputs, depth);
        }
        if depth > 32 {
            return None;
        }
        let goal = flux_ty::Predicate {
            self_ty: alias.args[0],
            trait_ref: flux_ty::TraitRef {
                trait_: alias.trait_,
                name: alias.trait_name,
                args: &alias.args[1..],
            },
        };
        if let Some(row) = self.family_projection_output(alias, inputs, owner) {
            return Some(row);
        }
        // A represented equality in the original caller environment supplies
        // the output row. Own type/const arguments remain part of alias identity.
        if owner.is_empty()
            && family.is_empty()
            && let Some((_, row)) = self.env_regions.iter().find(|(known, _)| *known == alias)
        {
            return Some(row.clone());
        }
        let selected = flux_solve::traits::select_definite_impl(self.items, self.sigs, goal);
        let mut assumptions = if self.body.owner.is_some() {
            self.caller_region_goals()
        } else {
            self.family_goals.clone()
        };
        if let TyKind::Opaque(opaque) = goal.self_ty.kind() {
            assumptions.extend(self.opaque_bound_goals(opaque, inputs.first()?)?);
        }
        // A caller's abstract owner does not establish the conditions of a
        // blanket impl. For example, `&T: IntoIterator` does not imply that
        // `&T: Iterator`, although the latter would select IntoIterator's
        // blanket impl. Keep the original abstract occurrence in that case.
        let abstract_owner =
            selected.is_none() && assumptions.iter().any(|known| known.predicate == goal);
        if abstract_owner
            && !self.impl_prove_super(
                &ImplRegionGoal {
                    predicate: goal,
                    lifetimes: owner.to_vec(),
                    vars: inputs.to_vec(),
                    bound: Vec::new(),
                },
                &assumptions,
                depth + 1,
            )
        {
            return None;
        }
        let chosen = if abstract_owner {
            None
        } else {
            selected.or_else(|| self.matching_impl(goal))
        };
        let Some((imp, header_args)) = chosen else {
            // An abstract projection keeps its occurrence inputs. Its declared
            // output lifetime bounds are applied separately, not invented from
            // a chosen impl or another occurrence of the same erased Ty.
            let mut row = owner.to_vec();
            row.extend(inputs.iter().flatten().copied());
            row.extend(family);
            row.extend(family_inputs.iter().flatten().copied());
            return Some(row);
        };
        if self.body.owner.is_none() {
            let requirement = ImplRegionGoal {
                predicate: goal,
                lifetimes: owner.to_vec(),
                vars: inputs.to_vec(),
                bound: Vec::new(),
            };
            if !self.impl_prove_super(&requirement, &self.family_goals.clone(), depth) {
                return None;
            }
        } else {
            self.prove(goal, inputs, owner, self.call_range, depth);
        }
        let binding = self.bind_impl(
            imp,
            &header_args,
            alias.args,
            inputs,
            owner,
            self.call_range,
            depth,
            None,
        )?;
        let impl_regions = self.sigs.regions.impls.get(imp)?;
        let template = impl_regions
            .families
            .iter()
            .find(|(name, _)| *name == alias.name)?
            .1
            .clone();
        if template.owner as usize != binding.lifetimes.len()
            || template.names.len() != binding.lifetimes.len() + family.len()
        {
            return None;
        }
        let mut lifetimes: Vec<_> = binding.lifetimes.iter().copied().collect::<Option<_>>()?;
        lifetimes.extend(family);
        let mut types = Vec::new();
        for (index, row) in binding.types.into_iter().enumerate() {
            match row {
                Some(row) => types.push(row),
                None if self.slots(header_args[index]) == 0 => types.push(Vec::new()),
                None => return None,
            }
        }
        // A zero-sized/scalar output still selects this impl. Its type-only
        // outlives predicates must not disappear with an empty output row.
        for prerequisite in &impl_regions.bounds {
            let header_map = |r| match r {
                Region::Static => Some(STATIC),
                Region::Param(i) => lifetimes.get(i as usize).copied(),
                _ => None,
            };
            match *prerequisite {
                Outlives::Region(a, b) => self.outlives(
                    header_map(a)?,
                    header_map(b)?,
                    Category::CallArgument,
                    self.call_range,
                ),
                Outlives::Type(ty, b) => {
                    let b = header_map(b)?;
                    for v in self.bound_slots(ty, &header_args, &types)? {
                        self.outlives(v, b, Category::CallArgument, self.call_range);
                    }
                    self.type_outlives(self.normalize(ty.subst(&header_args)), b, self.call_range);
                }
            }
        }
        types.extend(family_inputs.iter().cloned());
        let mut args = header_args.clone();
        args.extend(alias.family_args);
        let value = flux_solve::traits::impl_type(self.sigs, imp, alias.name)?;
        let mut map = |r| match r {
            Region::Static => Some(STATIC),
            Region::Param(i) => lifetimes.get(i as usize).copied(),
            _ => None,
        };
        let result = self.instantiate_sig(value, &template.value, &mut map, &args, &types)?;
        // This header and its lifetime requirements have now been proved.
        // Preserve its type substitution even when the erased selector could
        // not infer the impl's bounds from generic inputs on its own.
        let ty = value.subst(&args);
        if ty != Ty::alias(alias) && !self.env.iter().any(|known| known.alias == alias) {
            self.env.push(flux_ty::Projection { alias, ty });
        }
        Some(result)
    }
}

/// Check each family under universal owner and family binders, even when no
/// expression projects it. Neither declaration may silently add prerequisites.
pub(super) fn impl_family_values_match(
    items: &ItemTree,
    sigs: &Signatures,
    imp: flux_hir::ImplId,
) -> Option<bool> {
    let Some(trait_ref) = sigs.impl_traits.get(imp).copied() else {
        return Some(true);
    };
    let Some(block) = sigs.regions.impls.get(imp) else {
        return Some(true);
    };
    for definition in &items.impls[imp].types {
        let Some((_, decl)) = sigs
            .regions
            .traits
            .get(trait_ref.trait_)?
            .families
            .iter()
            .find(|(name, _)| *name == definition.name)
        else {
            continue;
        };
        let Some((_, actual)) = block
            .families
            .iter()
            .find(|(name, _)| *name == definition.name)
        else {
            continue;
        };
        let declaration = items.traits[trait_ref.trait_]
            .types
            .iter()
            .find(|f| f.name == definition.name)?;
        let bindings = declaration
            .bounds
            .iter()
            .flat_map(|b| b.path.args.iter().flatten())
            .map(|a| a.bindings.len())
            .sum::<usize>();
        let implicit_sized = sigs.builtins.sized.is_some_and(|sized| {
            !declaration.maybe_unsized
                && (!declaration.generics.lifetimes.is_empty()
                    || !declaration.generics.params.is_empty())
                && !sigs.traits.get(trait_ref.trait_).is_some_and(|signature| {
                    signature
                        .type_bounds
                        .iter()
                        .any(|(name, bound)| *name == definition.name && bound.trait_ == sized)
                })
        });
        if decl.output_traits.len() != declaration.bounds.len() + usize::from(implicit_sized)
            || decl.output_projections.len() != bindings
            || actual.value.len()
                != flux_ty::regions::slots(
                    items,
                    flux_solve::traits::impl_type(sigs, imp, definition.name)?,
                )
        {
            return None;
        }
        if decl.owner == 0
            && decl.names.is_empty()
            && definition.generics.params.is_empty()
            && decl.output_outlives.is_empty()
        {
            continue;
        }
        if decl.owner as usize > decl.names.len() || actual.owner as usize > actual.names.len() {
            return None;
        }
        if decl.names.len() - decl.owner as usize != actual.names.len() - actual.owner as usize {
            return Some(false);
        }
        for reverse in [false, true] {
            let body = Body::empty(None);
            let none = Requirements::default();
            let mut cx = Cx::new(&body, items, sigs, &none);
            let (mut lifetimes, mut env) = cx.impl_header_environment(imp)?;
            for index in actual.owner as usize..actual.names.len() {
                lifetimes.push(cx.new_universal(format!("'family{index}")));
            }
            let map_impl = |r| match r {
                Region::Static => Some(STATIC),
                Region::Param(index) => lifetimes.get(index as usize).copied(),
                _ => None,
            };
            let mut owner = Vec::new();
            for &r in &block.trait_lifetimes {
                owner.push(map_impl(r)?);
            }
            let mut declaration_lifetimes = owner.clone();
            declaration_lifetimes.extend(&lifetimes[actual.owner as usize..]);
            let map_decl = |r| match r {
                Region::Static => Some(STATIC),
                Region::Param(index) => declaration_lifetimes.get(index as usize).copied(),
                _ => None,
            };
            let self_ty = sigs.impls[imp];
            let mut args = vec![self_ty];
            args.extend(trait_ref.args);
            let header_count = items.impls[imp].generics.params.len();
            for (index, param) in definition.generics.params.iter().enumerate() {
                args.push(Ty::param((header_count + index) as u32, param.name));
            }
            let mut inputs =
                vec![cx.instantiate_sig(self_ty, &block.self_ty, &mut { map_impl }, &[], &[])?];
            for (&ty, row) in trait_ref.args.iter().zip(block.trait_args.iter()) {
                inputs.push(cx.instantiate_sig(ty, row, &mut { map_impl }, &[], &[])?);
            }
            inputs.extend(definition.generics.params.iter().map(|_| Vec::new()));
            let (assumptions, requirements) = if reverse {
                (&actual.outlives, &decl.outlives)
            } else {
                (&decl.outlives, &actual.outlives)
            };
            for assumption in assumptions {
                let (a, b) = match *assumption {
                    Outlives::Region(a, b) => (Some(a), b),
                    Outlives::Type(ty, b) => {
                        let mapped = if reverse { map_impl(b)? } else { map_decl(b)? };
                        let instance = if reverse { ty } else { ty.subst(&args) };
                        for component in flux_ty::regions::outlives_type_components(sigs, instance)
                        {
                            cx.known_types.push((component, mapped));
                        }
                        if !reverse {
                            for v in cx.bound_slots(ty, &args, &inputs)? {
                                cx.known_pairs.push((v, mapped));
                            }
                        }
                        continue;
                    }
                };
                let mapper = if reverse {
                    &map_impl as &dyn Fn(Region) -> Option<Var>
                } else {
                    &map_decl
                };
                cx.known_pairs.push((mapper(a?)?, mapper(b)?));
            }
            if let Some(schema) = sigs
                .impl_families
                .get(imp)
                .and_then(|families| families.iter().find(|(name, _)| *name == definition.name))
                .map(|(_, schema)| schema)
            {
                for (index, &predicate) in schema.predicates.iter().enumerate() {
                    let goal = cx.impl_region_goal(
                        predicate,
                        actual.bound_selfs.get(index).map_or(&[], |row| row),
                        actual
                            .bound_traits
                            .get(index)
                            .unwrap_or(&TraitRefRegions::default()),
                        &lifetimes,
                        &[],
                        &[],
                    )?;
                    if !cx.impl_super_goals(goal, &mut env, 0) {
                        return None;
                    }
                    cx.header_predicates.push(predicate);
                }
                for (projection, table) in schema.projections.iter().zip(actual.projections.iter())
                {
                    let row =
                        cx.instantiate_sig(projection.ty, table, &mut { map_impl }, &[], &[])?;
                    cx.env.push(*projection);
                    cx.env_regions.push((projection.alias, row));
                }
                cx.family_goals = env.clone();
            }
            cx.close_known();
            for requirement in requirements {
                let mapper = if reverse {
                    &map_decl as &dyn Fn(Region) -> Option<Var>
                } else {
                    &map_impl
                };
                match *requirement {
                    Outlives::Region(a, b) => cx.outlives(
                        mapper(a)?,
                        mapper(b)?,
                        Category::Boring,
                        definition.item_range,
                    ),
                    Outlives::Type(ty, b) => {
                        let b = mapper(b)?;
                        if reverse {
                            for v in cx.bound_slots(ty, &args, &inputs)? {
                                cx.outlives(v, b, Category::Boring, definition.item_range);
                            }
                        }
                        let ty = if reverse { ty.subst(&args) } else { ty };
                        cx.type_outlives(ty, b, definition.item_range);
                    }
                }
            }
            if !reverse {
                let value = flux_solve::traits::impl_type(sigs, imp, definition.name)?;
                let row = cx.instantiate_sig(value, &actual.value, &mut { map_impl }, &[], &[])?;
                if !cx.adt_declaration_uses(value, &row, &env)? {
                    return Some(false);
                }
                for &bound in &decl.output_outlives {
                    let target = map_decl(bound)?;
                    if cx.family_output_outlives(value, &row, target, &env) {
                        continue;
                    }
                    for &v in &row {
                        cx.outlives(v, target, Category::Boring, definition.item_range);
                    }
                    cx.type_outlives(value, target, definition.item_range);
                }
                for (bound, tables) in &decl.output_traits {
                    let mut vars = vec![row.clone()];
                    for (&ty, table) in bound.args.iter().zip(tables.args.iter()) {
                        vars.push(cx.instantiate_sig(
                            ty,
                            table,
                            &mut { map_decl },
                            &args,
                            &inputs,
                        )?);
                    }
                    let trait_lifetimes = tables
                        .lifetimes
                        .iter()
                        .map(|&r| map_decl(r))
                        .collect::<Option<_>>()?;
                    let goal = ImplRegionGoal {
                        predicate: bound.subst(&args).with_self(cx.normalize(value)),
                        lifetimes: trait_lifetimes,
                        vars,
                        bound: Vec::new(),
                    };
                    if !cx.impl_prove_super(&goal, &env, 0) {
                        return Some(false);
                    }
                }
                for (projection, table) in &decl.output_projections {
                    let expected = cx.instantiate_sig(
                        projection.ty,
                        table,
                        &mut { map_decl },
                        &args,
                        &inputs,
                    )?;
                    let alias = projection
                        .alias
                        .map_args(|ty| cx.normalize(ty.subst(&args)));
                    if alias.family_lifetimes != 0 || !alias.family_args.is_empty() {
                        return None;
                    }
                    let actual_ty = cx.normalize(Ty::alias(alias));
                    let expected_ty = cx.normalize(projection.ty.subst(&args));
                    if actual_ty != expected_ty {
                        return Some(false);
                    }
                    let self_vars = row.clone();
                    let mut alias_vars = vec![self_vars];
                    for &arg in &alias.args[1..] {
                        let n = cx.slots(arg);
                        if n != 0 {
                            return None;
                        }
                        alias_vars.push(Vec::new());
                    }
                    let actual = if alias.owner_lifetimes != 0 {
                        let (_, owner_table) = decl
                            .output_traits
                            .iter()
                            .find(|(bound, _)| bound.trait_ == alias.trait_)?;
                        let owner = owner_table
                            .lifetimes
                            .iter()
                            .map(|&r| map_decl(r))
                            .collect::<Option<Vec<_>>>()?;
                        cx.family_prerequisites(alias, &owner, &alias_vars, &[], &[])?;
                        cx.family_output(alias, &owner, &alias_vars, &[], &[], 0)?
                    } else {
                        cx.normalize_regions(alias, &alias_vars, 0)?
                    };
                    if actual.len() != expected.len() {
                        return None;
                    }
                    for (a, b) in actual.into_iter().zip(expected) {
                        cx.equate(a, b, Category::Boring, definition.item_range);
                    }
                }
            }
            let solution = cx.solve(&Points::new(&body), &[], &[]);
            if !cx.check_universal(&solution).is_empty()
                || !cx.check_type_tests(&solution).is_empty()
                || !cx.errors.is_empty()
            {
                return Some(false);
            }
        }
    }
    Some(true)
}

impl Cx<'_> {
    /// Bounds on a family are available only through its original owning
    /// predicate, with the owner and family lifetimes from this exact value.
    pub(super) fn caller_family_bound(
        &mut self,
        goal: flux_ty::Predicate,
        vars: &[Vec<Var>],
        trait_lifetimes: &[Var],
        range: TextRange,
    ) -> bool {
        let TyKind::Alias(alias) = goal.self_ty.kind() else {
            return false;
        };
        if !flux_ty::regions::has_projection_inputs(self.items, alias) || self.family_depth > 32 {
            return false;
        }
        self.family_depth += 1;
        let result = (|| -> Option<bool> {
            let row = vars.first()?;
            let mut next = 0;
            let owner = row.get(..alias.owner_lifetimes as usize)?.to_vec();
            next += owner.len();
            let mut inputs = Vec::new();
            for &ty in alias.args {
                let count = self.slots(ty) as usize;
                inputs.push(row.get(next..next + count)?.to_vec());
                next += count;
            }
            let family = row
                .get(next..next + alias.family_lifetimes as usize)?
                .to_vec();
            next += family.len();
            let mut family_inputs = Vec::new();
            for &ty in alias.family_args {
                let count = self.slots(ty) as usize;
                family_inputs.push(row.get(next..next + count)?.to_vec());
                next += count;
            }
            if next != row.len() {
                return None;
            }
            let lifetimes: Vec<_> = owner.iter().chain(&family).copied().collect();
            let types: Vec<_> = inputs.iter().chain(&family_inputs).cloned().collect();
            let bounds = self.declared_output_traits(alias, row, &lifetimes, &types, 0)?;
            self.close_known();
            let equalities = self.caller_goal_equalities(&bounds, goal, vars, trait_lifetimes)?;
            let owner_goal = flux_ty::Predicate {
                self_ty: alias.args[0],
                trait_ref: flux_ty::TraitRef {
                    trait_: alias.trait_,
                    name: alias.trait_name,
                    args: &alias.args[1..],
                },
            };
            if matches!(owner_goal.self_ty.kind(), TyKind::Opaque(_)) {
                // Opaque owners carry their contract at the occurrence, not
                // in the enclosing function's written where clauses.
                self.prove(owner_goal, &inputs, &owner, range, 0);
            } else if !self.caller_bound(owner_goal, &inputs, &owner, range) {
                return Some(false);
            }
            self.family_prerequisites(alias, &owner, &inputs, &family, &family_inputs)?;
            for (a, b) in equalities {
                self.equate(a, b, Category::CallArgument, range);
            }
            Some(true)
        })()
        .unwrap_or(false);
        self.family_depth -= 1;
        result
    }
}

impl Cx<'_> {
    /// Forward an associated equality on one family without retaining the
    /// temporary receiver borrow of an iterator adapter's `next` call.
    pub(super) fn family_projection_output(
        &mut self,
        alias: AliasTy,
        arg_vars: &[Vec<Var>],
        outer_owner: &[Var],
    ) -> Option<Vec<Var>> {
        let TyKind::Alias(owner_alias) = alias.args.first()?.kind() else {
            return None;
        };
        if !flux_ty::regions::has_projection_inputs(self.items, owner_alias)
            || self.family_depth > 32
        {
            return None;
        }
        let row = arg_vars.first()?;
        let mut next = 0;
        let owner = row.get(..owner_alias.owner_lifetimes as usize)?.to_vec();
        next += owner.len();
        let mut inputs = Vec::new();
        for &ty in owner_alias.args {
            let n = self.slots(ty) as usize;
            inputs.push(row.get(next..next + n)?.to_vec());
            next += n;
        }
        let family = row
            .get(next..next + owner_alias.family_lifetimes as usize)?
            .to_vec();
        next += family.len();
        let mut family_inputs = Vec::new();
        for &ty in owner_alias.family_args {
            let n = self.slots(ty) as usize;
            family_inputs.push(row.get(next..next + n)?.to_vec());
            next += n;
        }
        if next != row.len() {
            return None;
        }
        let contract = self
            .sigs
            .regions
            .traits
            .get(owner_alias.trait_)?
            .families
            .iter()
            .find(|(name, _)| *name == owner_alias.name)?
            .1
            .clone();
        let args = owner_alias.template_args();
        let (projection, table) = contract
            .output_projections
            .iter()
            .find(|(p, _)| p.alias.map_args(|ty| ty.subst(&args)) == alias)?;
        let lifetimes: Vec<_> = owner.iter().chain(&family).copied().collect();
        let types: Vec<_> = inputs.iter().chain(&family_inputs).cloned().collect();
        let mut map = |r| match r {
            Region::Static => Some(STATIC),
            Region::Param(i) => lifetimes.get(i as usize).copied(),
            _ => None,
        };
        if alias.owner_lifetimes as usize != outer_owner.len() {
            return None;
        }
        if !outer_owner.is_empty() {
            let (_, bound) = contract
                .output_traits
                .iter()
                .find(|(bound, _)| bound.trait_ == alias.trait_)?;
            if bound.lifetimes.len() != outer_owner.len() {
                return None;
            }
            for (&r, &actual) in bound.lifetimes.iter().zip(outer_owner) {
                self.equate(map(r)?, actual, Category::CallArgument, self.call_range);
            }
        }
        self.family_depth += 1;
        let result = self.instantiate_sig(projection.ty, table, &mut map, &args, &types);
        self.family_depth -= 1;
        result
    }
}

impl Cx<'_> {
    /// A unique blanket header can be selected on an abstract family, but its
    /// predicates are still proved below from the original caller contracts.
    pub(super) fn family_blanket_impl(
        &self,
        goal: flux_ty::Predicate,
    ) -> Option<(flux_hir::ImplId, Vec<Ty>)> {
        let TyKind::Alias(alias) = goal.self_ty.kind() else {
            return None;
        };
        if !flux_ty::regions::has_projection_inputs(self.items, alias) {
            return None;
        }
        let mut candidates = self
            .sigs
            .impl_index
            .impls_for(self.items, goal.trait_ref.trait_, goal.self_ty)
            .filter(|&id| matches!(self.sigs.impls[id].kind(), TyKind::Param(_)))
            .filter_map(|id| {
                flux_solve::traits::match_impl(self.items, self.sigs, id, goal)
                    .map(|args| (id, args))
            });
        let selected = candidates.next()?;
        candidates.next().is_none().then_some(selected)
    }
}
