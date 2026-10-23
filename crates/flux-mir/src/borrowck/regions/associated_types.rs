//! Declaration-time well-formedness of associated value projections.
use super::*;

/// Prove selection even when the normalized associated value has zero slots.
/// `None` retains an explicit unsupported boundary for missing input tables.
pub(crate) fn impl_associated_values_match(
    items: &ItemTree,
    sigs: &Signatures,
    imp: flux_hir::ImplId,
) -> Option<bool> {
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
    let solution = cx.solve(&Points::new(&body), &[]);
    Some(
        cx.check_universal(&solution).is_empty()
            && cx.check_type_tests(&solution).is_empty()
            && cx.errors.is_empty(),
    )
}

impl Cx<'_> {
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
        let mut env = Vec::new();
        for (k, &p) in self
            .sigs
            .impl_predicates
            .get(imp)
            .map_or(&[][..], |p| p)
            .iter()
            .enumerate()
        {
            let Some(goal) = self.impl_region_goal(
                p,
                block.bound_selfs.get(k).map_or(&[], |t| t),
                block
                    .bound_traits
                    .get(k)
                    .unwrap_or(&TraitRefRegions::default()),
                &lifetimes,
                &[],
                &[],
            ) else {
                continue;
            };
            self.impl_super_goals(goal, &mut env, 0);
        }
        for (p, table) in self.env.clone().iter().zip(block.projections.iter()) {
            if let Some(vars) = self.instantiate_sig(p.ty, table, &mut { map }, &[], &[]) {
                self.env_regions.push((p.alias, vars));
            }
        }
        Some((lifetimes, env))
    }

    #[allow(clippy::too_many_arguments)]
    fn associated_value_selections(
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
        if !self.impl_prove_super(
            &ImplRegionGoal {
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
            },
            env,
            0,
        ) {
            return false;
        }
        true
    }
}
