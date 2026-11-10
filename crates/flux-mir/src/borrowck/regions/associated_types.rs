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
            if let Some(vars) = self.instantiate_sig(p.ty, table, &mut { map }, &[], &[]) {
                self.env_regions.push((p.alias, vars));
            }
        }
        self.family_goals = env.clone();
        Some((lifetimes, env))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn associated_value_selections(
        &mut self,
        ty: Ty,
        selections: &[flux_ty::regions::ProjectionSelection],
        lifetimes: &[Var],
        env: &[ImplRegionGoal],
        used: &mut FxHashMap<AliasTy, usize>,
        depth: u32,
    ) -> bool {
        if depth > 32 {
            return false;
        }
        for child in ty.children() {
            if !self.associated_value_selections(child, selections, lifetimes, env, used, depth + 1)
            {
                return false;
            }
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
            let Some(v) = self.instantiate_sig(arg, table, &mut map, &[], &[]) else {
                return false;
            };
            let instance = self.normalize(arg);
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
        if !self.impl_prove_super(
            &ImplRegionGoal {
                lifetimes: owner_lifetimes,
                predicate: flux_ty::Predicate {
                    self_ty: self.normalize(alias.args[0]),
                    trait_ref: flux_ty::TraitRef {
                        trait_: alias.trait_,
                        name: alias.trait_name,
                        args: Ty::list(
                            &alias.args[1..]
                                .iter()
                                .map(|&t| self.normalize(t))
                                .collect::<Vec<_>>(),
                        ),
                    },
                },
                vars,
                bound: Vec::new(),
            },
            env,
            0,
        ) {
            return false;
        }
        true
    }
}
