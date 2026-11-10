//! A selected signature projection must be well formed before its RHS is used.
use super::*;

pub(crate) fn check_signature_projection_requirements(
    items: &ItemTree,
    sigs: &Signatures,
    first: u32,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for (function, data) in items.functions.iter().skip(first as usize) {
        let Some(regions) = sigs.regions.fns.get(function) else {
            continue;
        };
        let signature = &sigs.fns[function];
        let selected_method = matches!(data.owner, Some(flux_hir::Owner::Impl(imp))
            if sigs.impl_traits.get(imp).is_some());
        let memberships = flux_solve::traits::predicates_of(items, sigs, function);
        let parameter_contract = memberships.iter().any(|p| {
            sigs.trait_predicates
                .get(p.trait_ref.trait_)
                .is_some_and(|p| !p.is_empty())
        });
        let selection_contract = !(regions.selected_inputs.iter().all(|row| row.is_empty())
            && (selected_method
                || signature
                    .params
                    .iter()
                    .chain([&signature.ret])
                    .all(|ty| !ty.has_aliases())));
        if !parameter_contract && !selection_contract {
            continue;
        }
        let body = Body::empty(Some(function));
        let requirements = Requirements::default();
        let mut cx = Cx::new(&body, items, sigs, &requirements);
        if !selection_contract
            && !memberships
                .iter()
                .any(|&p| !cx.lifetime_trait_parameters(p, 0).is_empty())
        {
            continue;
        }
        cx.header_predicates = flux_solve::traits::elaborate(sigs, &memberships);
        cx.header_type_equalities();
        cx.new_universal("'static".into());
        let lifetimes: Vec<_> = (0..regions.names.len())
            .map(|i| cx.new_universal(format!("'{i}")))
            .collect();
        let map = |r| match r {
            Region::Static => Some(STATIC),
            Region::Param(i) => lifetimes.get(i as usize).copied(),
            Region::Erased | Region::Late(_) => None,
        };
        // These are the original explicit and signature-implied assumptions.
        // Selected impl prerequisites below are obligations, never new facts.
        for &bound in &regions.outlives {
            match bound {
                Outlives::Region(a, b) => {
                    if let (Some(a), Some(b)) = (map(a), map(b)) {
                        cx.known_pairs.push((a, b));
                    }
                }
                Outlives::Type(ty, b) => {
                    if let Some(b) = map(b) {
                        cx.known_types.push((ty, b));
                    }
                }
            }
        }
        cx.close_known();
        let mut env = cx.caller_region_goals();
        let mut complete = regions.selected_inputs.len() == signature.params.len() + 1
            || (regions.selected_inputs.is_empty()
                && signature
                    .params
                    .iter()
                    .chain([&signature.ret])
                    .all(|ty| !ty.has_aliases()));
        let mut projections = Vec::new();
        let mut bounds = Vec::new();
        if let Some(flux_hir::Owner::Impl(imp)) = data.owner
            && let Some(owner) = sigs.regions.impls.get(imp)
        {
            let count = sigs.impl_predicates.get(imp).map_or(0, |p| p.len());
            complete &= owner.bound_selfs.len() == count && owner.bound_traits.len() == count;
            bounds.extend(
                sigs.impl_predicates
                    .get(imp)
                    .into_iter()
                    .flatten()
                    .copied()
                    .zip(owner.bound_selfs.iter().cloned())
                    .zip(owner.bound_traits.iter().cloned())
                    .map(|((predicate, row), traits)| (predicate, row, traits, owner.names.len())),
            );
            projections.extend(
                sigs.impl_projections
                    .get(imp)
                    .into_iter()
                    .flatten()
                    .copied()
                    .zip(owner.projections.iter().cloned()),
            );
        }
        if let Some(flux_hir::Owner::Trait(owner)) = data.owner {
            let owner_count = sigs.regions.traits.get(owner).map_or(0, |r| r.names.len());
            complete &= owner_count == regions.owner as usize;
            if let Some((owner_lifetimes, (selfs, traits, equalities))) = lifetimes
                .get(..owner_count)
                .zip(flux_ty::regions::trait_bound_regions(items, sigs, owner))
            {
                for ((&predicate, row), bound) in sigs
                    .trait_predicates
                    .get(owner)
                    .into_iter()
                    .flatten()
                    .zip(selfs.iter())
                    .zip(traits.iter())
                {
                    bounds.push((predicate, row.clone(), bound.clone(), owner_lifetimes.len()));
                }
                projections.extend(
                    sigs.traits[owner]
                        .header_projections
                        .iter()
                        .copied()
                        .zip(equalities),
                );
            } else {
                complete = false;
            }
        }
        projections.extend(
            sigs.fn_projections
                .get(function)
                .into_iter()
                .flatten()
                .copied()
                .zip(regions.projections.iter().cloned()),
        );
        bounds.extend(
            sigs.fn_predicates
                .get(function)
                .into_iter()
                .flatten()
                .copied()
                .zip(regions.bound_selfs.iter().cloned())
                .zip(regions.bound_traits.iter().cloned())
                .map(|((predicate, row), traits)| (predicate, row, traits, lifetimes.len())),
        );
        let count = sigs.fn_predicates.get(function).map_or(0, |p| p.len());
        complete &= regions.bound_selfs.len() == count && regions.bound_traits.len() == count;
        let mut pending = Vec::new();
        // Establish every directly represented original assumption before
        // proving selections nested in another assumption's arguments.
        for (predicate, row, traits, count) in bounds {
            complete &= row.len() == flux_ty::regions::slots(items, predicate.self_ty)
                && traits.args.len() == predicate.trait_ref.args.len()
                && traits.lifetimes.len()
                    == items.traits[predicate.trait_ref.trait_]
                        .generics
                        .lifetimes
                        .len()
                && traits
                    .args
                    .iter()
                    .zip(predicate.trait_ref.args)
                    .all(|(row, &ty)| row.len() == flux_ty::regions::slots(items, ty));
            let goal = lifetimes.get(..count).and_then(|life| {
                cx.impl_region_goal_with_proof(predicate, &row, &traits, life, &[], &[], None, true)
            });
            if let Some(goal) = goal {
                complete &= cx.impl_super_goals(goal, &mut env, 0);
            } else {
                pending.push((predicate, row, traits, count));
            }
        }
        // Normalized assumptions are available to the value signature below,
        // but cannot retroactively justify another bound's input selection.
        let original = env.clone();
        for (predicate, row, traits, count) in pending {
            let goal = lifetimes.get(..count).and_then(|life| {
                cx.impl_region_goal_with_inputs(
                    predicate,
                    &row,
                    &traits,
                    life,
                    &[],
                    &[],
                    None,
                    true,
                    Some(&original),
                )
            });
            match goal {
                Some(goal) => {
                    complete &= cx.impl_super_goals(goal, &mut env, 0);
                }
                None => complete = false,
            }
        }
        cx.close_known();
        for goal in &env {
            complete &= cx.trait_parameters_hold(goal, &env, 0) == Some(true);
        }
        cx.family_goals = env.clone();
        for (projection, table) in projections {
            // A quantified equality is instantiated by its original input
            // occurrence. No concrete row exists at declaration scope.
            if cx
                .env_ranked
                .iter()
                .any(|scheme| scheme.alias == projection.alias)
            {
                continue;
            }
            if let Some(row) = cx.instantiate_sig(projection.ty, &table, &mut { map }, &[], &[]) {
                cx.env_regions.push((projection.alias, row));
            } else {
                complete = false;
            }
        }
        for (&ty, selections) in signature
            .params
            .iter()
            .chain([&signature.ret])
            .zip(regions.selected_inputs.iter())
        {
            complete &= selections.iter().all(|selection| {
                selection.lifetimes.len() == selection.alias.owner_lifetimes as usize
                    && selection.args.len() == selection.alias.args.len()
                    && selection
                        .args
                        .iter()
                        .zip(selection.alias.args)
                        .all(|(row, &arg)| row.len() == flux_ty::regions::slots(items, arg))
            });
            complete &= cx.associated_value_selections(
                ty,
                selections,
                &lifetimes,
                &env,
                &mut FxHashMap::default(),
                0,
            );
        }
        cx.close_known();
        let solution = cx.solve(&Points::new(&body), &[], &[]);
        if !complete
            || !cx.errors.is_empty()
            || !cx.check_universal(&solution).is_empty()
            || !cx.check_type_tests(&solution).is_empty()
        {
            diagnostics.push(Diagnostic {
                file: data.loc.file,
                range: data.ret_range,
                message: "associated type projection requirements are not satisfied by the function declaration".into(),
            });
        }
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrowed_trait_owner_rows_keep_the_method_owner_namespace() {
        let source = "trait Stream{type Out;}trait Append<'a,I:'a>where &'a I:Stream{fn append(x:&'a I)->Option<<&'a I as Stream>::Out>;}fn main(){}";
        let (_, root, hir) = flux_hir::lower_text("owner_rows.rs", source);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let (sigs, _) = flux_ty::signatures_checked(&hir.items, true, None);
        let owner = hir
            .items
            .traits
            .iter()
            .find(|(_, data)| data.loc.file == root && data.name.as_str() == "Append")
            .unwrap()
            .0;
        let rows = flux_ty::regions::trait_bound_regions(&hir.items, &sigs, owner).unwrap();
        assert_eq!(&*rows.0[0], &[Region::Param(0)]);
        // This metadata control is independent of the existing admission gate
        // for this lifetime-bearing trait's associated-output signature.
        assert!(
            check_signature_projection_requirements(&hir.items, &sigs, hir.items.core.functions)
                .is_empty()
        );
    }
}
