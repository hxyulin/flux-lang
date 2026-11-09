//! Opaque associated equalities use their original declaring bound occurrence.
use super::*;

struct OutputDomain {
    owner: Vec<Var>,
    inputs: Vec<Vec<Var>>,
    ty: Ty,
    output: Vec<Var>,
}

impl Cx<'_> {
    pub(super) fn opaque_equality_output(
        &mut self,
        opaque: flux_ty::OpaqueTy,
        alias: AliasTy,
        owner: &[Var],
        inputs: &[Vec<Var>],
    ) -> Option<Vec<Var>> {
        if !self.opaque_bound_metadata(opaque) {
            return None;
        }
        let own = inputs.first()?;
        if own.len() != self.slots(Ty::opaque(opaque)) as usize {
            return None;
        }
        let sig = self
            .sigs
            .opaques
            .get(opaque.function)?
            .get(opaque.index as usize)?;
        let metadata = self.sigs.regions.fns.get(opaque.function)?;
        let declaration = metadata.opaques.get(opaque.index as usize)?;
        let written = self.items.functions[opaque.function]
            .opaques
            .get(opaque.index as usize)?;
        let args = flux_solve::traits::opaque_args(opaque);
        let mut types = Vec::new();
        let mut next = declaration.captures.len();
        for &ty in opaque.args {
            let end = next.checked_add(self.slots(ty) as usize)?;
            types.push(own.get(next..end)?.to_vec());
            next = end;
        }
        if next != own.len() {
            return None;
        }
        types.push(own.clone());
        let mut lifetimes = vec![STATIC; metadata.names.len()];
        for (index, &capture) in declaration.captures.iter().enumerate() {
            if let Region::Param(slot) = capture {
                *lifetimes.get_mut(slot as usize)? = *own.get(index)?;
            }
        }
        // Typed projections follow source binding order. An erased alias alone
        // cannot select between Read<'a>::Out and Read<'b>::Out.
        let mut origins = Vec::new();
        for (bound_index, &origin) in sig.bound_origins.iter().enumerate() {
            let bound = written.bounds.get(origin as usize)?;
            let last = bound.path.segments.len().saturating_sub(1);
            for binding in bound
                .path
                .args(last)
                .into_iter()
                .flat_map(|args| args.bindings.iter())
            {
                if sig.projections.get(origins.len())?.alias.name != binding.name {
                    return None;
                }
                origins.push(bound_index);
            }
        }
        if origins.len() != sig.projections.len() {
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
        let env = self.caller_region_goals(false);
        let mut found = None;
        for (index, projection) in sig.projections.iter().enumerate() {
            if projection.subst(&args).alias != alias {
                continue;
            }
            let bound = declaration.bounds.get(origins[index])?;
            let owners = (|| {
                let original = self.impl_region_goal_with_inputs(
                    bound.predicate,
                    &[],
                    &bound.regions,
                    &lifetimes,
                    &args,
                    &types,
                    None,
                    false,
                    Some(&env),
                )?;
                let mut owners = Vec::new();
                self.impl_super_goals(original, &mut owners, 0)
                    .then_some(owners)
            })();
            // Complete the original bound/supertrait binder construction even
            // on failure before any equality or domain comparison reads it.
            self.close_known();
            let owners = owners?;
            for candidate in owners
                .iter()
                .filter(|candidate| candidate.predicate == goal)
            {
                let Some(equalities) = self.caller_goal_equalities(
                    std::slice::from_ref(candidate),
                    goal,
                    inputs,
                    owner,
                ) else {
                    continue;
                };
                // Quantified inputs are instantiated at this occurrence. They
                // are not the arbitrary universal variables of the declaration.
                let quantified = if flux_ty::fn_trait_kind(self.items, alias.trait_).is_none() {
                    let mut row = vec![None; candidate.bound.len()];
                    for (&source, &actual) in candidate.lifetimes.iter().zip(owner).chain(
                        candidate
                            .vars
                            .iter()
                            .zip(inputs)
                            .flat_map(|(a, b)| a.iter().zip(b)),
                    ) {
                        if let Some(slot) = candidate.bound.iter().position(|&v| v == source) {
                            row[slot].get_or_insert(actual);
                        }
                    }
                    Some(row)
                } else {
                    None
                };
                let row = self.opaque_projection_at(
                    opaque,
                    index,
                    own,
                    inputs.get(1).map_or(&[], Vec::as_slice),
                    quantified.as_deref(),
                );
                // Instantiating a declared output may select nested ranked
                // metadata. Close its relation on failure as well as success.
                self.close_known();
                let row = row?;
                let key = OutputDomain {
                    owner: candidate.lifetimes.clone(),
                    inputs: candidate.vars.clone(),
                    ty: self.normalize(projection.ty.subst(&args)),
                    output: row,
                };
                if let Some((old, _)) = &found {
                    if !self.opaque_output_domains_equal(old, &key) {
                        self.errors.push(Diagnostic::new(
                            self.call_range,
                            "opaque associated output occurrence is ambiguous",
                        ));
                        return None;
                    }
                } else {
                    found = Some((key, equalities));
                }
            }
        }
        let (domain, equalities) = found?;
        for (a, b) in equalities {
            self.equate(a, b, Category::CallArgument, self.call_range);
        }
        Some(domain.output)
    }
    fn opaque_output_domains_equal(&self, left: &OutputDomain, right: &OutputDomain) -> bool {
        if left.ty != right.ty
            || left.owner.len() != right.owner.len()
            || left.inputs.len() != right.inputs.len()
            || left.output.len() != right.output.len()
            || left
                .inputs
                .iter()
                .zip(&right.inputs)
                .any(|(a, b)| a.len() != b.len())
        {
            return false;
        }
        let equal = |a, b| {
            if self.callable_output_equivalent(a, b) {
                return true;
            }
            match (self.universal_of.get(&a), self.universal_of.get(&b)) {
                (Some(&a), Some(&b)) => self.known(a, b) && self.known(b, a),
                _ => false,
            }
        };
        left.owner
            .iter()
            .zip(&right.owner)
            .chain(
                left.inputs
                    .iter()
                    .flatten()
                    .zip(right.inputs.iter().flatten()),
            )
            .chain(left.output.iter().zip(&right.output))
            .all(|(&a, &b)| equal(a, b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_output_domain_equality_uses_only_existing_facts() {
        let (_, _, hir) = flux_hir::lower_text("main.rs", "fn main(){}");
        let typed = crate::check_file(&hir);
        let body = Body::empty(None);
        let requirements = Requirements::default();
        let mut cx = Cx::new(&body, &hir.items, &typed.sigs, &requirements);
        cx.new_universal("'static".into());
        let a = cx.new_universal("'a".into());
        let b = cx.new_universal("'b".into());
        cx.close_known();
        let domain = |owner| OutputDomain {
            owner: vec![owner],
            inputs: vec![vec![a, b]],
            ty: Ty::new(TyKind::Ref(Mutability::Not, Ty::int(flux_ty::IntTy::U32))),
            output: vec![owner],
        };
        let left = domain(a);
        let right = domain(b);
        assert!(cx.opaque_output_domains_equal(&left, &left));
        assert!(
            !cx.opaque_output_domains_equal(&left, &right),
            "prospective owner equalities are not established input facts"
        );
        assert!(!cx.opaque_output_domains_equal(&left, &domain(STATIC)));
        cx.known_pairs.extend([(a, b), (b, a)]);
        cx.close_known();
        assert!(cx.opaque_output_domains_equal(&left, &right));
        let x = cx.fresh();
        let y = cx.fresh();
        assert!(!cx.opaque_output_domains_equal(&domain(x), &domain(y)));
        cx.equate(x, y, Category::CallArgument, TextRange::default());
        assert!(cx.opaque_output_domains_equal(&domain(x), &domain(y)));
    }
    #[test]
    fn ranked_opaque_queries_close_the_original_universal_relation() {
        let (_, _, hir) =
            flux_hir::lower_text("main.rs", "fn make()->impl Fn(&u32)->u32{|v|*v}fn main(){}");
        let typed = crate::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let make = hir
            .items
            .functions
            .iter()
            .find(|(_, f)| f.name.as_str() == "make")
            .unwrap()
            .0;
        let TyKind::Opaque(opaque) = typed.sigs.fns[make].ret.kind() else {
            panic!("missing opaque output")
        };
        let sig = &typed.sigs.opaques[make][0];
        let args = flux_solve::traits::opaque_args(opaque);
        let goal = sig.predicates[0].subst(&args);
        let alias = sig.projections[0].subst(&args).alias;
        let body = Body::empty(None);
        let requirements = Requirements::default();
        let mut cx = Cx::new(&body, &hir.items, &typed.sigs, &requirements);
        cx.new_universal("'static".into());
        cx.close_known();
        let input = cx.fresh();
        cx.holes.insert(input);
        cx.prove(goal, &[vec![], vec![input]], &[], TextRange::default(), 0);
        assert!(cx.errors.is_empty(), "{:?}", cx.errors);
        assert!(cx.universal.len() > 1);
        assert_eq!(cx.known.len(), cx.universal.len() * cx.universal.len());
        for (index, &(var, _)) in cx.universal.iter().enumerate() {
            assert_eq!(cx.universal_of[&var], index as u32);
        }
        let before = cx.universal.len();
        assert_eq!(
            cx.opaque_equality_output(opaque, alias, &[], &[vec![], vec![input]]),
            Some(vec![])
        );
        assert!(cx.universal.len() > before);
        assert_eq!(cx.known.len(), cx.universal.len() * cx.universal.len());
        for (index, &(var, _)) in cx.universal.iter().enumerate() {
            assert_eq!(cx.universal_of[&var], index as u32);
        }
    }
}
