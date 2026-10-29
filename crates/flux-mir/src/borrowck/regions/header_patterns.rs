//! Match a projected header only from independently bound selection inputs.
use super::*;

impl Cx<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn projected_header_rows(
        &mut self,
        pattern: Ty,
        selections: &[flux_ty::regions::ProjectionSelection],
        impl_args: &[Ty],
        binding: &ImplBinding,
        env: &[ImplRegionGoal],
        depth: u32,
    ) -> Option<(Ty, Vec<Var>)> {
        let TyKind::Alias(alias) = pattern.kind() else {
            return None;
        };
        // A direct projection has one selection occurrence in this
        // header argument. Nested or family selections need richer binding;
        // their output must never be inverted to infer missing inputs.
        let [selection] = selections else {
            return None;
        };
        if selection.alias != alias
            || alias.family_lifetimes != 0
            || !alias.family_args.is_empty()
            || selection.lifetimes.len() != alias.owner_lifetimes as usize
            || selection.args.len() != alias.args.len()
            || alias.args.iter().any(|arg| arg.has_aliases())
        {
            return None;
        }
        let rows = binding
            .types
            .iter()
            .zip(impl_args)
            .map(|(row, &ty)| match row {
                Some(row) => Some(row.clone()),
                None if self.slots(ty) == 0 => Some(Vec::new()),
                None => None,
            })
            .collect::<Option<Vec<_>>>()?;
        let map = |region| match region {
            Region::Static => Some(STATIC),
            Region::Param(index) => binding.lifetimes.get(index as usize).copied().flatten(),
            Region::Erased | Region::Late(_) => None,
        };
        let mut inputs = Vec::new();
        let mut input_rows = Vec::new();
        let owner = selection
            .lifetimes
            .iter()
            .copied()
            .map(map)
            .collect::<Option<Vec<_>>>()?;
        for (&ty, table) in alias.args.iter().zip(selection.args.iter()) {
            let row = self.instantiate_sig(ty, table, &mut { map }, impl_args, &rows)?;
            let ty = self.normalize(ty.subst(impl_args));
            if row.len() != self.slots(ty) as usize {
                return None;
            }
            inputs.push(ty);
            input_rows.push(row);
        }
        let instance = AliasTy {
            args: Ty::list(&inputs),
            ..alias
        };
        let goal = ImplRegionGoal {
            predicate: flux_ty::Predicate {
                self_ty: *inputs.first()?,
                trait_ref: flux_ty::TraitRef {
                    trait_: alias.trait_,
                    name: alias.trait_name,
                    args: &instance.args[1..],
                },
            },
            vars: input_rows.clone(),
            lifetimes: owner,
            bound: Vec::new(),
        };
        if !self.impl_prove_super(&goal, env, depth + 1) {
            return None;
        }
        let output_rows = if alias.owner_lifetimes == 0 {
            self.normalize_regions(instance, &input_rows, depth + 1)?
        } else {
            // Retain the occurrence's owner binders while selecting the RHS;
            // erased normalization alone cannot recover these lifetimes.
            let table: Vec<_> = selection
                .lifetimes
                .iter()
                .chain(selection.args.iter().flatten())
                .copied()
                .collect();
            self.instantiate_sig(pattern, &table, &mut { map }, impl_args, &rows)?
        };
        let output = self.normalize(Ty::alias(instance));
        if output == Ty::alias(instance) {
            return None;
        }
        (output_rows.len() == self.slots(output) as usize).then_some((output, output_rows))
    }
}
