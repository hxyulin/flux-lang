//! Projection equality proofs retain each predicate's original input occurrence.
use super::*;
pub(super) type ProjectionInputRecipe = (flux_ty::Predicate, Box<[Region]>, TraitRefRegions);

impl Cx<'_> {
    pub(super) fn callable_output_recipe_present(
        &self,
        written: &flux_hir::Bound,
        predicate: flux_ty::Predicate,
        projections: &[flux_ty::Projection],
    ) -> bool {
        let args = written
            .path
            .args(written.path.segments.len().saturating_sub(1));
        if !args.is_some_and(|args| {
            args.parenthesized
                || args
                    .bindings
                    .iter()
                    .any(|binding| binding.name.as_str() == "Output")
        }) || flux_ty::fn_trait_kind(self.items, predicate.trait_ref.trait_).is_none()
        {
            return true;
        }
        projections.iter().any(|projection| {
            self.is_fn_output(projection.alias)
                && projection.alias.args.first() == Some(&predicate.self_ty)
                && projection.alias.args.get(1) == predicate.trait_ref.args.first()
        })
    }

    pub(super) fn callable_outputs_complete(
        &mut self,
        generics: &flux_hir::Generics,
        predicates: &[flux_ty::Predicate],
        projections: &[flux_ty::Projection],
        range: TextRange,
    ) -> bool {
        let has_written_output = generics.predicates.iter().any(|written| {
            written
                .bound
                .path
                .args(written.bound.path.segments.len().saturating_sub(1))
                .is_some_and(|args| {
                    args.parenthesized
                        || args
                            .bindings
                            .iter()
                            .any(|binding| binding.name.as_str() == "Output")
                })
        });
        let written_names: Vec<_> = generics
            .predicates
            .iter()
            .flat_map(|written| {
                written
                    .bound
                    .path
                    .args(written.bound.path.segments.len().saturating_sub(1))
                    .into_iter()
                    .flat_map(|args| args.bindings.iter().map(|binding| binding.name))
            })
            .collect();
        let complete = (!(has_written_output || !written_names.is_empty())
            || predicates.len() == generics.predicates.len())
            && written_names.len() <= projections.len()
            && written_names
                .iter()
                .zip(projections)
                .all(|(&name, projection)| name == projection.alias.name)
            && generics
                .predicates
                .iter()
                .zip(predicates)
                .all(|(written, &predicate)| {
                    self.callable_output_recipe_present(&written.bound, predicate, projections)
                });
        if !complete {
            self.errors.push(Diagnostic::new(
                range,
                "callable output metadata is incomplete",
            ));
        }
        complete
    }

    /// Source binding order is the occurrence identity. Two erased-equal
    /// aliases can originate in distinct predicates with different input loans.
    pub(super) fn declared_projection_input(
        &mut self,
        generics: &flux_hir::Generics,
        projections: &[flux_ty::Projection],
        index: usize,
        bounds: &[(flux_ty::Predicate, &[Region], TraitRefRegions)],
        range: TextRange,
    ) -> Option<ProjectionInputRecipe> {
        let written: Vec<_> = generics.predicates.iter().map(|p| &p.bound).collect();
        self.written_projection_input(&written, projections, index, bounds, range)
    }

    pub(super) fn written_projection_input(
        &mut self,
        written: &[&flux_hir::Bound],
        projections: &[flux_ty::Projection],
        index: usize,
        bounds: &[(flux_ty::Predicate, &[Region], TraitRefRegions)],
        range: TextRange,
    ) -> Option<ProjectionInputRecipe> {
        let mut next = 0;
        for (bound_index, written) in written.iter().enumerate() {
            let last = written.path.segments.len().saturating_sub(1);
            for binding in written
                .path
                .args(last)
                .into_iter()
                .flat_map(|args| args.bindings.iter())
            {
                if next == index {
                    if projections
                        .get(index)
                        .is_some_and(|p| p.alias.name == binding.name)
                        && let Some((predicate, own, rows)) = bounds.get(bound_index)
                    {
                        return Some((*predicate, Box::from(*own), rows.clone()));
                    }
                    self.errors.push(Diagnostic::new(
                        range,
                        "associated input occurrence metadata is incomplete",
                    ));
                    return None;
                }
                next += 1;
            }
        }
        // Repeated identical metadata does not create a new source occurrence.
        if let Some(projection) = projections.get(index)
            && let Some(original) = projections[..index].iter().position(|p| p == projection)
        {
            return self.written_projection_input(written, projections, original, bounds, range);
        }
        self.errors.push(Diagnostic::new(
            range,
            "associated input occurrence metadata is incomplete",
        ));
        None
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn projection_input_goal(
        &mut self,
        alias: AliasTy,
        recipe: &ProjectionInputRecipe,
        lifetimes: &[Var],
        args: &[Ty],
        arg_vars: &[Vec<Var>],
    ) -> Option<ImplRegionGoal> {
        let (predicate, own, rows) = recipe;
        // The original bound's subject has already passed ordinary membership.
        // Its written row remains authoritative even when substitution produces
        // an abstract associated subject; do not reselect that subject here.
        let goal = self.impl_region_goal_with_proof(
            *predicate, own, rows, lifetimes, args, arg_vars, None, true,
        )?;
        let (&self_ty, trait_args) = alias.args.split_first()?;
        let owns = |owner: &ImplRegionGoal| {
            owner.predicate.trait_ref.trait_ == alias.trait_
                && owner.predicate.self_ty == self_ty
                && owner.predicate.trait_ref.args == trait_args
        };
        // A directly declared equality already has its complete original
        // owning predicate. Unrelated supertrait projections must not erase
        // that occurrence or demand an invented second set of input rows.
        if owns(&goal) {
            return Some(goal);
        }
        let mut owners = Vec::new();
        if !self.impl_super_goals(goal, &mut owners, 0) {
            return None;
        }
        let mut found: Option<ImplRegionGoal> = None;
        for owner in owners {
            if !owns(&owner) {
                continue;
            }
            if found
                .as_ref()
                .is_some_and(|old| old.vars != owner.vars || old.lifetimes != owner.lifetimes)
            {
                return None;
            }
            found = Some(owner);
        }
        found
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn callable_output_rows(
        &mut self,
        predicate: flux_ty::Predicate,
        self_table: &[Region],
        rows: &TraitRefRegions,
        lifetimes: &[Var],
        args: &[Ty],
        arg_vars: &[Vec<Var>],
        range: TextRange,
    ) -> Option<Vec<Vec<Var>>> {
        let caller_constraints = self.constraints.len();
        let mut output = None;
        for (projection, table) in &self.call_projections {
            if !self.is_fn_output(projection.alias)
                || projection.alias.args.first() != Some(&predicate.self_ty)
                || projection.alias.args.get(1) != predicate.trait_ref.args.first()
            {
                continue;
            }
            if output
                .as_ref()
                .is_some_and(|(old, row)| old != projection || row != table)
            {
                self.errors.push(Diagnostic::new(
                    range,
                    "callable output bound occurrence is ambiguous",
                ));
                return None;
            }
            output = Some((*projection, table.clone()));
        }
        let count = rows
            .args
            .iter()
            .flat_map(|row| row.iter())
            .filter_map(|region| match region {
                Region::Late(index) => Some(*index as usize + 1),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        let fresh = (0..count)
            .map(|_| {
                let var = self.fresh();
                self.holes.insert(var);
                var
            })
            .collect();
        let previous = std::mem::replace(&mut self.late_inputs, fresh);
        let result = (|| {
            let mut actual_rows = Vec::new();
            for (index, ty) in std::iter::once(predicate.self_ty)
                .chain(predicate.trait_ref.args.iter().copied())
                .enumerate()
            {
                let table = if index == 0 {
                    self_table
                } else {
                    rows.args.get(index - 1)?
                };
                let vars = self.bound_slots_in(ty, table, lifetimes, args, arg_vars)?;
                if vars.len() != self.slots(self.normalize(ty.subst(args))) as usize {
                    return None;
                }
                actual_rows.push(vars);
            }
            let inputs = actual_rows.get(1).map_or(&[][..], Vec::as_slice);
            let input_ty = self.normalize(predicate.trait_ref.args.first()?.subst(args));
            let mut implied = Vec::new();
            flux_ty::regions::implied_outlives(
                self.items,
                self.sigs,
                input_ty,
                inputs,
                &mut implied,
            );
            for obligation in implied {
                if let Outlives::Region(a, b) = obligation
                    && self.holes.contains(&b)
                {
                    self.hole_pairs.insert((a, b));
                }
            }
            let bound: Vec<_> = inputs
                .iter()
                .copied()
                .filter(|var| self.holes.contains(var))
                .collect::<FxHashSet<_>>()
                .into_iter()
                .collect();
            if !bound.is_empty() {
                let implied: Vec<_> = self
                    .hole_pairs
                    .iter()
                    .copied()
                    .filter(|&(a, b)| {
                        (bound.contains(&a) || !self.holes.contains(&a))
                            && (bound.contains(&b) || !self.holes.contains(&b))
                    })
                    .collect();
                self.record_pointer_check_at(self.vars, bound, implied, range, caller_constraints);
            }
            self.late_inputs = self.late_slots(predicate.trait_ref.args[0], inputs, args, arg_vars);
            // Angle-bracket Fn<Args> in the trusted forwarding impls need not
            // constrain Output. Every written equality, including an empty
            // scalar/unit row, must retain its own recipe when present.
            let Some((projection, table)) = output else {
                return Some(actual_rows);
            };
            if table.len() != flux_ty::regions::slots(self.items, projection.ty) as usize {
                return None;
            }
            let TyKind::Alias(alias) = Ty::alias(projection.alias).subst(args).kind() else {
                return None;
            };
            let alias = alias.map_args(|ty| self.normalize(ty));
            let expected = self.bound_slots_in(projection.ty, &table, lifetimes, args, arg_vars)?;
            let expected_count = self.slots(self.normalize(projection.ty.subst(args))) as usize;
            if expected.len() != expected_count {
                return None;
            }
            // Keep source recipes and rows while comparing canonical input
            // keys. A projection equality may identify U::T with a caller F;
            // that never replaces the original output's lifetime mapping.
            let candidates: Vec<_> = self
                .env_regions
                .iter()
                .filter(|(source, _)| source.map_args(|ty| self.normalize(ty)) == alias)
                .cloned()
                .collect();
            let mut actual = None;
            for (source, mut vars) in candidates {
                if let Some((_, recipe)) = self
                    .env_late
                    .iter()
                    .find(|(candidate, _)| *candidate == source)
                {
                    if recipe.len() != vars.len() {
                        return None;
                    }
                    for (var, region) in vars.iter_mut().zip(recipe.iter()) {
                        if let Region::Late(index) = *region {
                            *var = *inputs.get(index as usize)?;
                        }
                    }
                }
                if actual.as_ref().is_some_and(|old| old != &vars) {
                    return None;
                }
                actual = Some(vars);
            }
            let actual = actual.or_else(|| {
                let mut source = alias.args[0];
                let mut own = actual_rows.first()?.as_slice();
                while let TyKind::Ref(_, inner) = source.kind() {
                    source = inner;
                    own = own.get(1..)?;
                }
                let TyKind::Tuple(input_types) = alias.args.get(1)?.kind() else {
                    return None;
                };
                match source.kind() {
                    TyKind::Closure(id, list) => {
                        self.closure_goal(
                            id,
                            list,
                            Some(own),
                            Some(inputs),
                            Some(&expected),
                            range,
                        );
                        Some(expected.clone())
                    }
                    TyKind::FnDef(_) => self.function_output_with_obligations(
                        source,
                        input_types,
                        inputs,
                        FunctionUse::CallableEquality,
                        None,
                    ),
                    TyKind::FnPtr(_) | TyKind::Dynamic(_) => {
                        let (signature, own) = match source.kind() {
                            TyKind::FnPtr(signature) => (signature, own),
                            TyKind::Dynamic(object) => {
                                (object.callable_signature()?, own.get(1..)?)
                            }
                            _ => unreachable!(),
                        };
                        let mut at = 0;
                        let mut rows = Vec::new();
                        for &input in input_types {
                            let count = self.slots(input) as usize;
                            rows.push(inputs.get(at..at + count)?.to_vec());
                            at += count;
                        }
                        if at != inputs.len() {
                            return None;
                        }
                        self.pointer_equality_inputs(signature, own, input_types, &rows, range)
                    }
                    _ => self.normalize_regions(alias, &actual_rows, 0),
                }
            });
            let Some(actual) = actual else {
                return (expected_count == 0).then_some(actual_rows);
            };
            if actual.len() != expected.len() {
                return None;
            }
            // Output equality cannot solve a required universal from the
            // result. Its identity must already follow from input binders.
            for (&actual, &expected) in actual.iter().zip(&expected) {
                let rigid = self.holes.contains(&actual)
                    || self.holes.contains(&expected)
                    || inputs
                        .iter()
                        .copied()
                        .filter(|input| self.holes.contains(input))
                        .any(|input| {
                            self.callable_output_equivalent(actual, input)
                                || self.callable_output_equivalent(expected, input)
                        });
                if rigid && !self.callable_output_equivalent(actual, expected) {
                    return None;
                }
            }
            for (actual, expected) in actual.into_iter().zip(expected) {
                self.equate(actual, expected, Category::CallArgument, range);
            }
            Some(actual_rows)
        })();
        self.late_inputs = previous;
        if result.is_none() {
            self.errors.push(Diagnostic::new(
                range,
                "callable output lifetime proof is not represented",
            ));
        }
        result
    }
    pub(super) fn callable_output_equivalent(&self, a: Var, b: Var) -> bool {
        let reaches = |from, to| {
            let mut pending = vec![from];
            let mut seen = FxHashSet::default();
            while let Some(var) = pending.pop() {
                if var == to {
                    return true;
                }
                if !seen.insert(var) {
                    continue;
                }
                pending.extend(
                    self.constraints
                        .iter()
                        .filter(|edge| edge.sup == var)
                        .map(|edge| edge.sub),
                );
                pending.extend(
                    self.hole_pairs
                        .iter()
                        .filter(|&&(source, _)| source == var)
                        .map(|&(_, target)| target),
                );
            }
            false
        };
        reaches(a, b) && reaches(b, a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equality_input_lookup_keeps_direct_and_inherited_occurrence_rows() {
        let source = "trait Own<T>{type Output;}trait Child<A,B>:Own<A>+Own<B>{}fn main(){}";
        let (_, _, hir) = flux_hir::lower_text("equality_input_rows.rs", source);
        let (sigs, diagnostics) = flux_ty::signatures_checked(&hir.items, true, None);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let (own, own_data) = hir
            .items
            .traits
            .iter()
            .find(|(_, t)| t.name.as_str() == "Own")
            .unwrap();
        let (child, child_data) = hir
            .items
            .traits
            .iter()
            .find(|(_, t)| t.name.as_str() == "Child")
            .unwrap();
        let reference = Ty::new(TyKind::Ref(Mutability::Not, Ty::int(flux_ty::IntTy::U32)));
        let alias = flux_solve::traits::alias_of(
            &hir.items,
            flux_ty::TraitRef {
                trait_: own,
                name: own_data.name,
                args: Ty::list(&[reference]),
            },
            Ty::UNIT,
            flux_intern::Symbol::intern("Output"),
        );
        let body = Body::empty(None);
        let requirements = Requirements::default();
        let mut cx = Cx::new(&body, &hir.items, &sigs, &requirements);
        cx.new_universal("'static".into());
        let a = cx.new_universal("'a".into());
        let b = cx.new_universal("'b".into());
        let parameter = |index| Ty::param(index, flux_intern::kw::SelfUpper);
        let direct = (
            flux_ty::Predicate {
                self_ty: parameter(0),
                trait_ref: flux_ty::TraitRef {
                    trait_: own,
                    name: own_data.name,
                    args: Ty::list(&[parameter(1)]),
                },
            },
            Box::default(),
            TraitRefRegions {
                args: vec![Box::default()].into(),
                ..TraitRefRegions::default()
            },
        );
        let goal = cx
            .projection_input_goal(
                alias,
                &direct,
                &[],
                &[Ty::UNIT, reference],
                &[vec![], vec![a]],
            )
            .unwrap();
        assert_eq!(goal.vars, vec![vec![], vec![a]]);
        assert!(
            cx.projection_input_goal(
                alias,
                &direct,
                &[],
                &[Ty::UNIT, reference],
                &[vec![], vec![]]
            )
            .is_none()
        );
        let inherited = (
            flux_ty::Predicate {
                self_ty: parameter(0),
                trait_ref: flux_ty::TraitRef {
                    trait_: child,
                    name: child_data.name,
                    args: Ty::list(&[parameter(1), parameter(2)]),
                },
            },
            Box::default(),
            TraitRefRegions {
                args: vec![Box::default(), Box::default()].into(),
                ..TraitRefRegions::default()
            },
        );
        let args = [Ty::UNIT, reference, reference];
        let goal = cx
            .projection_input_goal(alias, &inherited, &[], &args, &[vec![], vec![a], vec![a]])
            .unwrap();
        assert_eq!(goal.vars, vec![vec![], vec![a]]);
        assert!(
            cx.projection_input_goal(alias, &inherited, &[], &args, &[vec![], vec![a], vec![b]])
                .is_none()
        );
    }

    #[test]
    fn missing_equality_occurrence_metadata_is_not_reconstructed() {
        let source = "trait Own<T>{type Output;}fn check<R:Own<A,Output=A>,A>(){}fn main(){}";
        let (_, root, hir) = flux_hir::lower_text("missing_equality_input.rs", source);
        let (sigs, diagnostics) = flux_ty::signatures_checked(&hir.items, true, None);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let (function, data) = hir
            .items
            .functions
            .iter()
            .find(|(_, f)| f.loc.file == root && f.name.as_str() == "check")
            .unwrap();
        let projections = sigs.fn_projections.get(function).unwrap();
        let metadata = &sigs.regions.fns[function];
        let bounds: Vec<_> = sigs.fn_predicates[function]
            .iter()
            .copied()
            .zip(metadata.bound_selfs.iter())
            .zip(metadata.bound_traits.iter())
            .map(|((predicate, own), row)| (predicate, &**own, row.clone()))
            .collect();
        let body = Body::empty(None);
        let requirements = Requirements::default();
        let mut cx = Cx::new(&body, &hir.items, &sigs, &requirements);
        assert!(
            cx.declared_projection_input(
                &data.generics,
                projections,
                0,
                &bounds,
                TextRange::default()
            )
            .is_some()
        );
        assert!(
            cx.declared_projection_input(&data.generics, projections, 0, &[], TextRange::default())
                .is_none()
        );
        assert!(
            cx.errors
                .iter()
                .any(|error| error.message == "associated input occurrence metadata is incomplete")
        );
    }
}
