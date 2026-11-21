//! Declaration-time well-formedness of associated value projections.
use super::*;

/// Prove selection even when the normalized associated value has zero slots.
/// `None` retains an explicit unsupported boundary for missing input tables.
pub(crate) fn impl_associated_values_match(
    items: &ItemTree,
    sigs: &Signatures,
    imp: flux_hir::ImplId,
) -> Option<bool> {
    if !families::impl_family_values_match(items, sigs, imp)? {
        return Some(false);
    }
    let Some(block) = sigs.regions.impls.get(imp) else {
        return Some(true);
    };
    let values = sigs.impl_types.get(imp).map_or(&[][..], |v| v);
    if !values.iter().any(|(_, ty)| ty.has_aliases()) {
        return Some(true);
    }
    let body = Body::empty(None);
    let none = Requirements::default();
    let mut cx = Cx::new(&body, items, sigs, &none);
    let (lifetimes, env) = cx.impl_header_environment(imp)?;
    for &(name, ty) in values {
        if items.impls[imp].types.iter().any(|t| {
            t.name == name && (!t.generics.params.is_empty() || !t.generics.lifetimes.is_empty())
        }) {
            continue;
        }
        let selections = block
            .type_selections
            .iter()
            .find(|(n, _)| *n == name)
            .map_or(&[][..], |(_, s)| s);
        if !cx.associated_value_selections(
            ty,
            block
                .types
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, row)| row.as_ref()),
            selections,
            &lifetimes,
            &env,
            &mut FxHashMap::default(),
            0,
        ) {
            return None;
        }
    }
    cx.close_known();
    let solution = cx.solve(&Points::new(&body), &[], &[]);
    Some(
        cx.check_universal(&solution).is_empty()
            && cx.check_type_tests(&solution).is_empty()
            && cx.errors.is_empty(),
    )
}

impl Cx<'_> {
    pub(super) fn check_impl_const_regions(
        &mut self,
        id: flux_hir::ConstId,
        imp: flux_hir::ImplId,
        lifetimes: &[Var],
    ) {
        let items = self.items;
        let sigs = self.sigs;
        let Some(&trait_ref) = sigs.impl_traits.get(imp) else {
            return;
        };
        let konst = &items.consts[id];
        let Some(required) = konst
            .name
            .and_then(|name| items.traits[trait_ref.trait_].konst(items, name))
        else {
            return;
        };
        let Some(&source) = sigs.consts.get(id) else {
            return;
        };
        let Some(&template) = sigs.consts.get(required) else {
            return;
        };
        let args: Vec<_> = std::iter::once(sigs.impls[imp])
            .chain(trait_ref.args.iter().copied())
            .collect();
        let Some(block) = sigs.regions.impls.get(imp) else {
            return;
        };
        // The trait's signature can name associated types defined by this
        // very impl. Use the checked RHS of each ordinary associated type.
        let map = |region| match region {
            Region::Static => Some(STATIC),
            Region::Param(i) => lifetimes.get(i as usize).copied(),
            _ => None,
        };
        let env_start = self.env.len();
        let regions_start = self.env_regions.len();
        for (name, table) in &block.types {
            let Some(ty) = flux_solve::traits::impl_type(sigs, imp, *name) else {
                continue;
            };
            let alias = flux_solve::traits::alias_of(items, trait_ref, args[0], *name);
            if alias.owner_lifetimes != 0
                || items.impls[imp].types.iter().any(|item| {
                    item.name == *name
                        && (!item.generics.lifetimes.is_empty() || !item.generics.params.is_empty())
                })
            {
                continue;
            }
            if let Some(vars) = self.instantiate_sig(ty, table, &mut |r| map(r), &[], &[])
                && vars.len() == self.slots(self.normalize(ty)) as usize
            {
                self.env.push(flux_ty::Projection { alias, ty });
                self.env_regions.push((alias, vars));
            }
        }
        let target = self.normalize(template.subst(&args));
        let normalized = self.normalize(source);
        if self.slots(target) == 0
            && self.slots(normalized) == 0
            && flux_solve::fn_regions::same_binders(items, normalized, target)
        {
            self.env.truncate(env_start);
            self.env_regions.truncate(regions_start);
            return;
        }
        let range = konst.ty_range;
        let rows = (|| {
            if block.trait_args.len() != trait_ref.args.len() {
                return None;
            }
            let mut vars = Vec::with_capacity(args.len());
            for (&ty, row) in args
                .iter()
                .zip(std::iter::once(&block.self_ty).chain(block.trait_args.iter()))
            {
                let values = self.instantiate_sig(ty, row, &mut |r| map(r), &[], &[])?;
                if values.len() != self.slots(self.normalize(ty)) as usize {
                    return None;
                }
                vars.push(values);
            }
            let owner = block
                .trait_lifetimes
                .iter()
                .copied()
                .map(map)
                .collect::<Option<Vec<_>>>()?;
            let mut map_trait = |region| match region {
                Region::Static => Some(STATIC),
                Region::Param(i) => owner.get(i as usize).copied(),
                _ => None,
            };
            let found = self.instantiate_sig(
                source,
                sigs.regions.consts.get(id)?,
                &mut |r| map(r),
                &[],
                &[],
            )?;
            let expected = self.instantiate_sig(
                template,
                sigs.regions.consts.get(required)?,
                &mut map_trait,
                &args,
                &vars,
            )?;
            (found.len() == self.slots(self.normalize(source)) as usize
                && expected.len() == self.slots(target) as usize)
                .then_some((found, expected))
        })();
        if let Some((found, expected)) = rows {
            self.relate_types(
                source,
                &found,
                target,
                &expected,
                Variance::Covariant,
                Category::Boring,
                range,
            );
        } else {
            self.errors.push(Diagnostic::new(
                range,
                "implemented constant lifetime arguments are not represented",
            ));
        }
        self.env.truncate(env_start);
        self.env_regions.truncate(regions_start);
    }

    pub(super) fn associated_const_value(
        &mut self,
        id: flux_hir::ConstId,
        args: &[Ty],
        written: &flux_ty::regions::TypeArgRegions,
        range: TextRange,
    ) -> Option<(Ty, Vec<Var>)> {
        let items = self.items;
        let sigs = self.sigs;
        let Some(flux_hir::Owner::Trait(owner)) = items.consts[id].owner else {
            return None;
        };
        let header = sigs.regions.traits.get(owner)?;
        let owner_goal = flux_solve::traits::trait_self_predicate(items, owner);
        if args.len() != owner_goal.trait_ref.args.len() + 1 {
            return None;
        }
        let lifetimes = self.fresh_list(header.names.len() as u32);
        let types = self.instantiated_type_vars(args);
        for (index, row) in written {
            let vars = if *index == flux_ty::regions::TRAIT_LIFETIMES_ARG {
                &lifetimes
            } else {
                types.get(*index as usize)?
            };
            if row.len() != vars.len() {
                return None;
            }
            for (&var, &region) in vars.iter().zip(row.iter()) {
                let declared = match region {
                    Region::Static => Some(STATIC),
                    Region::Param(i) => self.universal.get(1 + i as usize).map(|&(v, _)| v),
                    Region::Erased => continue,
                    Region::Late(_) => None,
                }?;
                self.equate(var, declared, Category::CallArgument, range);
            }
        }
        for (&ty, vars) in args.iter().zip(&types) {
            let ty = self.normalize(ty);
            self.adt_obligations(ty, vars, range);
            self.require_written_type_wf(ty, vars, Category::CallArgument, range);
        }
        let map = |r| match r {
            Region::Static => Some(STATIC),
            Region::Param(i) => lifetimes.get(i as usize).copied(),
            _ => None,
        };
        for &requirement in &header.outlives {
            match requirement {
                Outlives::Region(a, b) => {
                    self.outlives(map(a)?, map(b)?, Category::CallArgument, range);
                }
                Outlives::Type(ty, b) => {
                    let row = self.instantiate_sig(ty, &[], &mut |r| map(r), args, &types)?;
                    let ty = self.normalize(ty.subst(args));
                    if row.len() != self.slots(ty) as usize {
                        return None;
                    }
                    self.stored_type_outlives(ty, &row, map(b)?, Category::CallArgument, range);
                }
            }
        }
        self.prove(owner_goal.subst(args), &types, &lifetimes, range, 0);
        let scheme = flux_ty::regions::trait_bound_schemes(items, sigs, owner)?;
        let bounds: Vec<_> = scheme
            .predicates
            .iter()
            .copied()
            .zip(scheme.subjects.iter())
            .zip(scheme.traits.iter().cloned())
            .map(|((predicate, row), regions)| (predicate, row.as_ref(), regions))
            .collect();
        let projections = &sigs.traits.get(owner)?.header_projections;
        if projections.len() != scheme.projections.len() {
            return None;
        }
        let previous = std::mem::replace(
            &mut self.call_projections,
            projections
                .iter()
                .copied()
                .zip(scheme.projections.iter().cloned())
                .collect(),
        );
        for (predicate, row, regions) in &bounds {
            self.prove_bound(*predicate, row, regions, &lifetimes, args, &types, range, 0);
        }
        self.call_projections = previous;
        for (index, (&projection, row)) in projections
            .iter()
            .zip(scheme.projections.iter())
            .enumerate()
        {
            let input =
                self.declared_projection_input(&scheme.schema, projections, index, &bounds, range);
            self.prove_projection(
                projection,
                row,
                input.as_ref(),
                &lifetimes,
                args,
                &types,
                range,
            );
        }
        let template = *sigs.consts.get(id)?;
        let row = self.instantiate_sig(
            template,
            sigs.regions.consts.get(id)?,
            &mut |r| map(r),
            args,
            &types,
        )?;
        let value = self.normalize(template.subst(args));
        (row.len() == self.slots(value) as usize).then_some((value, row))
    }

    /// A region-free output may follow from a quantified original header even
    /// when the projection's selection inputs contain references. Match those
    /// inputs before using the equality; never install an erased alias globally.
    pub(super) fn header_region_free_output(
        &mut self,
        alias: AliasTy,
        wanted: &ImplRegionGoal,
        env: &[ImplRegionGoal],
    ) -> Option<Ty> {
        if flux_ty::regions::has_declared_projection_inputs(alias) {
            return None;
        }
        fn type_only(cx: &Cx<'_>, ty: Ty) -> bool {
            flux_ty::regions::slots(cx.items, ty) == 0
                && ty.children().all(|child| type_only(cx, child))
        }
        for known in env {
            let declaration = self.sigs.traits.get(known.predicate.trait_ref.trait_)?;
            let args: Vec<_> = std::iter::once(known.predicate.self_ty)
                .chain(known.predicate.trait_ref.args.iter().copied())
                .collect();
            for projection in &declaration.header_projections {
                let instantiated = projection.subst(&args);
                if instantiated.alias != alias {
                    continue;
                }
                let output = self.normalize(instantiated.ty);
                if !type_only(self, output) {
                    continue;
                }
                let candidate = self.impl_region_goal(
                    flux_ty::Predicate {
                        self_ty: projection.alias.args[0],
                        trait_ref: flux_ty::TraitRef {
                            trait_: projection.alias.trait_,
                            name: projection.alias.trait_name,
                            args: &projection.alias.args[1..],
                        },
                    },
                    &[],
                    &TraitRefRegions::default(),
                    &known.lifetimes,
                    &args,
                    &known.vars,
                )?;
                if candidate.vars.len() != wanted.vars.len()
                    || candidate.lifetimes.len() != wanted.lifetimes.len()
                    || candidate
                        .vars
                        .iter()
                        .zip(&wanted.vars)
                        .any(|(a, b)| a.len() != b.len())
                {
                    continue;
                }
                let mut bound = FxHashMap::default();
                let mut equalities = Vec::new();
                for (a, b) in candidate
                    .lifetimes
                    .iter()
                    .copied()
                    .zip(wanted.lifetimes.iter().copied())
                    .chain(
                        candidate
                            .vars
                            .iter()
                            .zip(&wanted.vars)
                            .flat_map(|(a, b)| a.iter().copied().zip(b.iter().copied())),
                    )
                {
                    if known.bound.contains(&a) {
                        if let Some(previous) = bound.insert(a, b) {
                            equalities.push((previous, b));
                        }
                    } else {
                        equalities.push((a, b));
                    }
                }
                if !self.caller_equalities_fit(&equalities) {
                    continue;
                }
                for (a, b) in equalities {
                    self.equate(a, b, Category::Boring, TextRange::default());
                }
                return Some(output);
            }
        }
        None
    }

    /// Supertrait equalities are consequences of original header assumptions,
    /// not of the trait currently being implemented. Only type-only facts need
    /// no additional syntax-to-region tables here.
    pub(super) fn header_type_equalities(&mut self) {
        fn type_only(items: &ItemTree, ty: Ty) -> bool {
            flux_ty::regions::slots(items, ty) == 0
                && ty.children().all(|child| type_only(items, child))
        }
        for projection in
            flux_solve::traits::implied_projections(self.sigs, &self.header_predicates)
        {
            if type_only(
                self.items,
                Ty::alias(projection.alias.map_args(|ty| self.normalize(ty))),
            ) && type_only(self.items, self.normalize(projection.ty))
                && !self.env.iter().any(|known| known.alias == projection.alias)
            {
                self.env.push(projection);
                // These header consequences have no region slots on either
                // side. Keep the empty output row as well as the type equality,
                // so declaration checks need not select an abstract receiver.
                self.env_regions.push((projection.alias, Vec::new()));
            }
        }
    }

    /// Integration contract: call once on a fresh `Cx`. Install only the original
    /// impl header's projection, predicate and outlives assumptions. Return
    /// universal header lifetimes and elaborated predicate goals; callers add
    /// obligations, never assumptions.
    pub(super) fn impl_header_environment(
        &mut self,
        imp: flux_hir::ImplId,
    ) -> Option<(Vec<Var>, Vec<ImplRegionGoal>)> {
        let block = self.sigs.regions.impls.get(imp)?;
        self.env = self
            .sigs
            .impl_projections
            .get(imp)
            .map_or(Vec::new(), |p| p.to_vec());
        self.header_predicates = self
            .sigs
            .impl_predicates
            .get(imp)
            .map_or(Vec::new(), |predicates| {
                flux_solve::traits::elaborate(self.sigs, predicates)
            });
        self.header_type_equalities();
        self.new_universal("'static".into());
        let lifetimes: Vec<_> = (0..block.names.len())
            .map(|i| self.new_universal(format!("'{i}")))
            .collect();
        let map = |r| match r {
            Region::Static => Some(STATIC),
            Region::Param(i) => lifetimes.get(i as usize).copied(),
            Region::Erased | Region::Late(_) => None,
        };
        for &p in &block.outlives {
            match p {
                Outlives::Region(a, b) => self.known_pairs.push((map(a)?, map(b)?)),
                Outlives::Type(t, b) => self.known_types.push((t, map(b)?)),
            }
        }
        self.assume_projection_bounds(&block.projection_bounds, map);
        // Elaborating header predicates can instantiate a selected family.
        // Its proof may query universals, so close the original header facts
        // before adding any obligations from that selection.
        self.close_known();
        let written = self
            .sigs
            .impl_predicates
            .get(imp)
            .into_iter()
            .flatten()
            .copied()
            .enumerate()
            .map(|(index, predicate)| {
                (
                    predicate,
                    block.bound_selfs.get(index).cloned().unwrap_or_default(),
                    block.bound_traits.get(index).cloned().unwrap_or_default(),
                )
            })
            .collect();
        let env = self.original_region_goals(written, &lifetimes);
        // Callable header binders can extend the universal region set.
        self.close_known();
        for (p, table) in self.env.clone().iter().zip(block.projections.iter()) {
            let late = table.iter().any(|region| matches!(region, Region::Late(_)));
            let mut inputs = &[][..];
            if late {
                if !self.is_fn_output(p.alias) {
                    continue;
                }
                // Keep the header's quantified input row. The output proof
                // substitutes the current occurrence's inputs for it.
                let mut occurrences = env
                    .iter()
                    .filter(|goal| {
                        goal.predicate.trait_ref.trait_ == p.alias.trait_
                            && goal.predicate.self_ty == p.alias.args[0]
                            && goal.predicate.trait_ref.args == &p.alias.args[1..]
                    })
                    .filter_map(|goal| {
                        let row = goal.vars.get(1)?;
                        table
                            .iter()
                            .all(|region| match *region {
                                Region::Late(index) => row
                                    .get(index as usize)
                                    .is_some_and(|var| goal.bound.contains(var)),
                                _ => true,
                            })
                            .then_some(row.as_slice())
                    });
                let Some(row) = occurrences.next() else {
                    continue;
                };
                if occurrences.any(|other| other != row) {
                    continue;
                }
                inputs = row;
            }
            let mut output_region = |region| match region {
                Region::Late(index) => inputs.get(index as usize).copied(),
                _ => map(region),
            };
            if let Some(vars) = self.instantiate_sig(p.ty, table, &mut output_region, &[], &[]) {
                self.env_regions.push((p.alias, vars));
                if late {
                    self.env_late.push((p.alias, table.clone()));
                }
            }
        }
        self.family_goals = env.clone();
        Some((lifetimes, env))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn associated_value_selections(
        &mut self,
        ty: Ty,
        table: Option<&[Region]>,
        selections: &[flux_ty::regions::ProjectionSelection],
        lifetimes: &[Var],
        env: &[ImplRegionGoal],
        used: &mut FxHashMap<AliasTy, usize>,
        depth: u32,
    ) -> bool {
        self.associated_value_selections_with_args(
            ty,
            table,
            selections,
            lifetimes,
            &[],
            &[],
            env,
            used,
            depth,
        )
    }

    /// Replay a selected impl's written occurrences with its exact bound
    /// lifetimes and substituted generic rows. The selection keys stay in
    /// declaration form so repeated erased-equal occurrences remain distinct.
    #[expect(
        clippy::too_many_arguments,
        reason = "selection replay keeps declaration tables and selected occurrence rows separate"
    )]
    pub(super) fn associated_value_selections_with_args(
        &mut self,
        ty: Ty,
        table: Option<&[Region]>,
        selections: &[flux_ty::regions::ProjectionSelection],
        lifetimes: &[Var],
        args: &[Ty],
        arg_vars: &[Vec<Var>],
        env: &[ImplRegionGoal],
        used: &mut FxHashMap<AliasTy, usize>,
        depth: u32,
    ) -> bool {
        if depth > 32 {
            return false;
        }
        if !args.is_empty() && matches!(ty.kind(), TyKind::Param(_)) {
            return self.instantiated_aliases_proven(ty, args, arg_vars, env, depth + 1);
        }
        if let TyKind::FnPtr(signature) = ty.kind()
            && let Some(recipe) = signature.regions.filter(|r| !r.projections.is_empty())
        {
            let Some(table) = table else {
                return false;
            };
            let mut map = |r| match r {
                Region::Static => Some(STATIC),
                Region::Param(i) => lifetimes.get(i as usize).copied(),
                _ => None,
            };
            let Some(own) = self.instantiate_sig(ty, table, &mut map, args, arg_vars) else {
                return false;
            };
            // The recipe proves the original projection, but an expanded
            // row has the selected output's shape, which can add slots.
            let signature = match self
                .normalize(if args.is_empty() { ty } else { ty.subst(args) })
                .kind()
            {
                TyKind::FnPtr(signature) => signature,
                _ => return false,
            };
            let bound: Vec<_> = (0..recipe.bound)
                .map(|i| self.new_universal(format!("'pointer{i}")))
                .collect();
            self.close_known();
            return self
                .instantiate_pointer_with_env(signature, &own, &bound, env)
                .is_some();
        }
        let (children, mut at): (Vec<_>, usize) = match ty.kind() {
            TyKind::Adt(adt, args) => (
                args.to_vec(),
                self.items.structs[adt.strukt].generics.lifetimes.len(),
            ),
            TyKind::Dynamic(object) => (
                flux_ty::regions::object_region_types(self.items, object).collect(),
                flux_ty::regions::object_own_slots(self.items, object),
            ),
            TyKind::Ref(_, inner) => (vec![inner], 1),
            TyKind::Array(inner, _) => (vec![inner], 0),
            _ => (ty.children().collect(), 0),
        };
        for child in children {
            let end = at + flux_ty::regions::slots(self.items, child);
            let row = table.and_then(|row| row.get(at..end));
            if !self.associated_value_selections_with_args(
                child,
                row,
                selections,
                lifetimes,
                args,
                arg_vars,
                env,
                used,
                depth + 1,
            ) {
                return false;
            }
            at = end;
        }
        let TyKind::Alias(alias) = ty.kind() else {
            return true;
        };
        // Consume each syntax occurrence once. An alias/default may hide an
        // additional occurrence with the same erased type but missing tables;
        // that occurrence must not borrow another occurrence's input regions.
        let count = used.entry(alias).or_default();
        let selection = selections.iter().filter(|s| s.alias == alias).nth(*count);
        *count += 1;
        let mut vars = Vec::new();
        let mut map = |r| match r {
            Region::Static => Some(STATIC),
            Region::Param(i) => lifetimes.get(i as usize).copied(),
            Region::Erased | Region::Late(_) => None,
        };
        for (k, &arg) in alias.args.iter().enumerate() {
            let table = selection.and_then(|s| s.args.get(k)).map_or(&[][..], |t| t);
            let Some(v) = self.instantiate_sig(arg, table, &mut map, args, arg_vars) else {
                return false;
            };
            let instance = self.normalize(if args.is_empty() {
                arg
            } else {
                arg.subst(args)
            });
            if v.len() != self.slots(instance) as usize {
                return false;
            }
            vars.push(v);
        }
        let owner = selection.map_or(&[][..], |selection| &selection.lifetimes);
        if owner.len() != alias.owner_lifetimes as usize {
            return false;
        }
        let Some(owner_lifetimes) = owner.iter().copied().map(&mut map).collect::<Option<_>>()
        else {
            return false;
        };
        self.impl_prove_super(
            &ImplRegionGoal {
                lifetimes: owner_lifetimes,
                predicate: flux_ty::Predicate {
                    self_ty: self.normalize(if args.is_empty() {
                        alias.args[0]
                    } else {
                        alias.args[0].subst(args)
                    }),
                    trait_ref: flux_ty::TraitRef {
                        trait_: alias.trait_,
                        name: alias.trait_name,
                        args: Ty::list(
                            &alias.args[1..]
                                .iter()
                                .map(|&t| {
                                    self.normalize(if args.is_empty() { t } else { t.subst(args) })
                                })
                                .collect::<Vec<_>>(),
                        ),
                    },
                },
                vars,
                bound: Vec::new(),
            },
            env,
            0,
        )
    }
}
