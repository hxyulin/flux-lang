//! Scoped function-pointer binder proofs and per-call instantiation.
use super::{Category, Cx, FunctionUse, STATIC, UniversalRelationMode, Var};
use crate::Place;
use flux_hir::Diagnostic;
use flux_syntax::TextRange;
use flux_ty::regions::{CallbackBinder, FnRegions, Region, Variance};
use flux_ty::{FnPtrRegions, FnRegion, Ty, TyKind};
use rustc_hash::FxHashSet;

/// Deferred universal proof for an indirect callback invocation.
pub(super) struct PointerCheck {
    pub before: u32,
    pub bound: Vec<Var>,
    pub implied: Vec<(Var, Var)>,
    input_wf: Vec<(Var, Var)>,
    pub range: TextRange,
}

fn record_pointer_check(
    checks: &mut Vec<PointerCheck>,
    before: Var,
    mut bound: Vec<Var>,
    mut input_wf: Vec<(Var, Var)>,
    mut implied: Vec<(Var, Var)>,
    range: TextRange,
) {
    bound.sort_unstable();
    bound.dedup();
    input_wf.sort_unstable();
    input_wf.dedup();
    implied.sort_unstable();
    implied.dedup();
    // A diagnostic location observes a proof; it does not introduce a new
    // binder. Reuse the first scope and input assumptions for this domain.
    if let Some(old) = checks
        .iter_mut()
        .find(|old| old.bound == bound && old.input_wf == input_wf)
    {
        old.before = old.before.min(before);
    } else {
        checks.push(PointerCheck {
            before,
            bound,
            implied,
            input_wf,
            range,
        });
    }
}

fn supported_item_pointer_binder(source: &FnRegions, target: &FnPtrRegions) -> bool {
    match source.callback_binder {
        CallbackBinder::Independent => true,
        CallbackBinder::Unclassified => false,
        CallbackBinder::SignatureBound => {
            if source.owner != 0
                || source.has_bounds
                || target.free == 0
                || !target.types.is_empty()
                || !target.projections.is_empty()
                || source.selected_inputs.iter().any(|row| !row.is_empty())
                || source.params.len() + 1 != target.inputs_output.len()
            {
                return false;
            }
            // A predicate can fix a named lifetime without fixing the
            // pointer's elided inputs. All named source lifetimes must be
            // represented and stay outside the pointer binder.
            let mut mapped = vec![false; source.names.len()];
            for (source_row, target_row) in source
                .params
                .iter()
                .map(AsRef::as_ref)
                .chain(std::iter::once(source.ret.as_ref()))
                .zip(target.inputs_output)
            {
                if source_row.len() != target_row.len() {
                    return false;
                }
                for (&source_region, &target_region) in source_row.iter().zip(*target_row) {
                    if let Region::Param(index) = source_region {
                        let Some(name) = source.names.get(index as usize) else {
                            return false;
                        };
                        if name.name.is_none() {
                            continue;
                        }
                        if !matches!(target_region, FnRegion::Free(_) | FnRegion::Static) {
                            return false;
                        }
                        mapped[index as usize] = true;
                    }
                }
            }
            source
                .names
                .iter()
                .zip(mapped)
                .all(|(name, mapped)| name.name.is_none() || mapped)
        }
    }
}

impl Cx<'_> {
    /// Expand bound-dependent output recipes in this caller's proof context.
    pub(super) fn instantiate_pointer(
        &mut self,
        signature: flux_ty::FnSigTy,
        own: &[Var],
        bound: &[Var],
    ) -> Option<Vec<Vec<Var>>> {
        let env = self.caller_region_goals();
        self.instantiate_pointer_with_env(signature, own, bound, &env)
    }

    pub(super) fn instantiate_pointer_with_env(
        &mut self,
        signature: flux_ty::FnSigTy,
        own: &[Var],
        bound: &[Var],
        env: &[super::ImplRegionGoal],
    ) -> Option<Vec<Vec<Var>>> {
        flux_ty::regions::instantiate_fn_pointer_with(
            self.items,
            signature,
            own,
            bound,
            STATIC,
            |alias, inputs| {
                self.proven_family_output(alias, inputs, env, 0)?;
                let source: Vec<_> = (0..inputs.len() as u32)
                    .map(flux_ty::regions::Region::Param)
                    .collect();
                let mut map = |r| match r {
                    flux_ty::regions::Region::Static => Some(STATIC),
                    flux_ty::regions::Region::Param(i) => inputs.get(i as usize).copied(),
                    _ => None,
                };
                self.instantiate_family(
                    alias,
                    &source,
                    &mut 0,
                    &mut map,
                    &[],
                    &Vec::<Vec<Var>>::new(),
                )
            },
        )
    }

    /// Finish a scoped higher-ranked proof. Older inference regions cannot
    /// capture its placeholders. An older region required to outlive every
    /// admitted placeholder must outlive its declared upper bounds, or static
    /// when the input signature provides none. This preserves body inference
    /// without allowing a pointer conversion to generalize captured borrows.
    pub(super) fn finish_pointer_proof(
        &mut self,
        before: u32,
        bound: &[Var],
        implied: &[(Var, Var)],
        universal: usize,
        known: usize,
        range: TextRange,
    ) {
        let mut edges = vec![Vec::new(); self.vars as usize];
        for c in &self.constraints {
            edges[c.sup as usize].push(c.sub);
        }
        for &(a, b) in implied {
            edges[a as usize].push(b);
        }
        let mut reverse = vec![Vec::new(); edges.len()];
        for (a, list) in edges.iter().enumerate() {
            for &b in list {
                reverse[b as usize].push(a as Var);
            }
        }
        let reachable = |start: Var, edges: &[Vec<Var>]| {
            let mut seen = vec![false; edges.len()];
            let mut pending = vec![start];
            while let Some(v) = pending.pop() {
                if std::mem::replace(&mut seen[v as usize], true) {
                    continue;
                }
                pending.extend(&edges[v as usize]);
            }
            seen
        };
        let allowed = |start: Var, end: Var| {
            let mut seen = FxHashSet::default();
            let mut pending = vec![start];
            while let Some(v) = pending.pop() {
                if !seen.insert(v) {
                    continue;
                }
                if v == end {
                    return true;
                }
                pending.extend(implied.iter().filter_map(|&(a, b)| (a == v).then_some(b)));
            }
            false
        };
        let mut invalid = !self.caller_equalities_fit_before(
            &[],
            universal,
            UniversalRelationMode::ClosureRequirements,
        );
        let mut promote = Vec::new();
        for &placeholder in bound {
            let below = reachable(placeholder, &edges);
            let above = reachable(placeholder, &reverse);
            if bound.iter().any(|&other| {
                other != placeholder && below[other as usize] && !allowed(placeholder, other)
            }) {
                invalid = true;
            }
            if (0..before).any(|old| below[old as usize] && !allowed(placeholder, old)) {
                invalid = true;
            }
            let mut upper: Vec<_> = implied
                .iter()
                .filter_map(|&(a, b)| (a < before && allowed(b, placeholder)).then_some(a))
                .collect();
            // Static is a redundant upper whenever input well-formedness
            // already bounds this placeholder by a nonstatic caller region.
            // Another quantified placeholder is not a fixed caller upper.
            // Keep all admitted input edges; do not strengthen that bounded
            // domain into a requirement to outlive static as well.
            if upper
                .iter()
                .any(|&region| region != STATIC && !bound.contains(&region))
            {
                upper.retain(|&region| region != STATIC);
            }
            // Covering one admitted fixed upper suffices. A deferred proof
            // can see ordinary inference slots created by a later call, but
            // cannot use that call's placeholders to establish coverage.
            let ordinary = |region: Var| {
                !bound.contains(&region)
                    && !self.holes.contains(&region)
                    && !self.retired_pointer_placeholders.contains(&region)
                    && (region < before || !self.universal_of.contains_key(&region))
            };
            let mut covered = vec![false; edges.len()];
            let mut pending: Vec<_> = upper
                .iter()
                .copied()
                .filter(|&region| ordinary(region))
                .collect();
            while let Some(region) = pending.pop() {
                if std::mem::replace(&mut covered[region as usize], true) {
                    continue;
                }
                pending.extend(
                    reverse[region as usize]
                        .iter()
                        .copied()
                        .filter(|&parent| ordinary(parent)),
                );
            }
            for old in 1..before {
                if above[old as usize] && !allowed(old, placeholder) && !covered[old as usize] {
                    if upper.is_empty() {
                        promote.push((old, STATIC));
                    } else {
                        for &upper in &upper {
                            promote.push((old, upper));
                        }
                    }
                }
            }
        }
        for (a, b) in promote {
            self.outlives(a, b, Category::CallArgument, range);
        }
        if invalid {
            self.errors.push(Diagnostic::new(
                range,
                "function pointer is not general enough",
            ));
        }
        for &(var, _) in &self.universal[universal..] {
            self.universal_of.remove(&var);
        }
        self.universal.truncate(universal);
        self.known_pairs.truncate(known);
        self.close_known();
        for &var in bound {
            self.holes.remove(&var);
            self.retired_pointer_placeholders.insert(var);
        }
    }

    pub(super) fn pointer_proof_inputs(
        &mut self,
        target: flux_ty::FnSigTy,
        own: &[Var],
        bound: &[Var],
    ) -> Option<Vec<Vec<Var>>> {
        let rows = self.instantiate_pointer(target, own, bound)?;
        // Reification compares independent binders, not calls on concrete
        // references. Input well-formedness must not change output links.
        self.close_known();
        Some(rows)
    }

    pub(super) fn pointer_subtype(
        &mut self,
        source: flux_ty::FnSigTy,
        source_own: &[Var],
        target: flux_ty::FnSigTy,
        target_own: &[Var],
        range: TextRange,
    ) {
        if source.abi != target.abi
            || source.is_variadic != target.is_variadic
            || (source.is_unsafe && !target.is_unsafe)
            || source.params().len() != target.params().len()
        {
            self.errors.push(Diagnostic::new(
                range,
                "incompatible function pointer signature",
            ));
            return;
        }
        let target = if target.regions.is_none()
            && target.inputs_output.iter().all(|&ty| self.slots(ty) == 0)
        {
            let rows = vec![Vec::new(); target.inputs_output.len()];
            flux_ty::FnSigTy {
                regions: Some(flux_ty::FnPtrRegions::intern(0, 0, &rows, &[])),
                ..target
            }
        } else {
            target
        };
        let Some(r) = target.regions else {
            self.errors.push(Diagnostic::new(
                range,
                "function pointer region metadata is not supported yet",
            ));
            return;
        };
        let before = self.vars;
        let universal = self.universal.len();
        let known = self.known_pairs.len();
        let bound: Vec<_> = (0..r.bound)
            .map(|i| self.new_universal(format!("'pointer{i}")))
            .collect();
        let prepared = self.pointer_proof_inputs(target, target_own, &bound);
        let valid = if let Some(rows) = prepared {
            if let Some(output) = self.pointer_inputs(
                source,
                source_own,
                target.params(),
                &rows[..target.params().len()],
                range,
            ) {
                self.relate_types(
                    source.ret(),
                    &output,
                    target.ret(),
                    rows.last().unwrap(),
                    Variance::Covariant,
                    Category::CallArgument,
                    range,
                );
                true
            } else {
                false
            }
        } else {
            false
        };
        if !valid {
            self.errors.push(Diagnostic::new(
                range,
                "function pointer region metadata is not supported yet",
            ));
        }
        self.finish_pointer_proof(before, &bound, &[], universal, known, range);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn pointer_from_item(
        &mut self,
        function: flux_hir::FunctionId,
        args: &[Ty],
        target: flux_ty::FnSigTy,
        own: &[Var],
        written: Option<&flux_ty::regions::TypeArgRegions>,
        captured: Option<&[Var]>,
        range: TextRange,
    ) {
        let Some(r) = target.regions else {
            if (written.is_none()
                && captured.is_none()
                && args
                    .iter()
                    .any(|&ty| flux_ty::regions::slots(self.items, ty) != 0))
                || self
                    .item_instantiation(
                        function,
                        args,
                        written,
                        FunctionUse::Reification,
                        range,
                        captured.unwrap_or_default(),
                    )
                    .is_none()
            {
                self.errors.push(Diagnostic::new(
                    range,
                    "function item pointer region provenance is not supported yet",
                ));
            }
            return;
        };
        // Owner lifetimes belong to the function item. Allocate them outside
        // the pointer binder so the leak check keeps them fixed across calls.
        let fixed_owner = if let Some(captured) = captured {
            captured.to_vec()
        } else if r.bound == 0 {
            Vec::new()
        } else {
            let count = self.sigs.regions.fns.get(function).map_or(0, |r| r.owner);
            self.fresh_list(count)
        };
        let before = self.vars;
        let universal = self.universal.len();
        let known = self.known_pairs.len();
        let bound: Vec<_> = (0..r.bound)
            .map(|i| self.new_universal(format!("'pointer{i}")))
            .collect();
        let prepared = self.pointer_proof_inputs(target, own, &bound);
        let mut valid = false;
        if let Some(rows) = prepared {
            let inputs: Vec<_> = rows[..target.params().len()]
                .iter()
                .flatten()
                .copied()
                .collect();
            let ret = rows.last().unwrap();
            if crate::fn_method_kind(self.items, function).is_some()
                && let Some(&closure) = args.first()
                && let TyKind::Closure(id, list) = closure.kind()
            {
                let closure = flux_ty::ClosureArgs::split(list);
                if let TyKind::Tuple(params) = closure.params().kind()
                    && params.len() == target.params().len()
                {
                    let mut closure_inputs = Vec::new();
                    for ((&source, source_vars), &dest) in
                        target.params().iter().zip(&rows).zip(params)
                    {
                        let count = self.slots(dest);
                        let vars = self.fresh_list(count);
                        self.relate_types(
                            source,
                            source_vars,
                            dest,
                            &vars,
                            Variance::Covariant,
                            Category::CallArgument,
                            range,
                        );
                        closure_inputs.extend(vars);
                    }
                    let count = self.slots(closure.ret());
                    let result = self.fresh_list(count);
                    for &var in &bound {
                        self.holes.insert(var);
                    }
                    self.closure_goal(
                        id,
                        list,
                        Some(&[]),
                        Some(&closure_inputs),
                        Some(&result),
                        range,
                    );
                    self.relate_types(
                        closure.ret(),
                        &result,
                        target.ret(),
                        ret,
                        Variance::Covariant,
                        Category::CallArgument,
                        range,
                    );
                    valid = true;
                }
            } else if let Some(regions) = self.sigs.regions.fns.get(function) {
                // Type arguments carrying regions still need written
                // provenance. Only the method's own late input loans can
                // be inferred independently inside a pointer binder.
                if (written.is_some()
                    || captured.is_some()
                    || !args
                        .iter()
                        .any(|&t| flux_ty::regions::slots(self.items, t) != 0))
                    && (r.bound == 0 || supported_item_pointer_binder(regions, r))
                {
                    let signature = self.sigs.fns[function].subst(args);
                    let data = &self.items.functions[function];
                    let sig = flux_ty::FnSigTy {
                        is_variadic: data.is_variadic,
                        abi: data.abi,
                        ..flux_ty::FnSigTy::new(&signature.params, signature.ret, data.is_unsafe)
                    };
                    let ty = Ty::new(TyKind::FnDef(flux_ty::FnDefTy {
                        function,
                        name: data.name,
                        args: Ty::list(args),
                        sig,
                    }));
                    let old = std::mem::replace(&mut self.call_range, range);
                    if let Some(output) = self.function_output_with_obligations(
                        ty,
                        target.params(),
                        &inputs,
                        FunctionUse::Reification,
                        written,
                        &fixed_owner,
                    ) {
                        self.relate_types(
                            signature.ret,
                            &output,
                            target.ret(),
                            ret,
                            Variance::Covariant,
                            Category::CallArgument,
                            range,
                        );
                        valid = true;
                    }
                    self.call_range = old;
                }
            }
        }
        if !valid {
            self.errors.push(Diagnostic::new(
                range,
                "function item pointer region provenance is not supported yet",
            ));
        }
        self.finish_pointer_proof(before, &bound, &[], universal, known, range);
    }

    /// Rechecking the same rigid domain must not mistake an earlier proof's
    /// fresh formals for external captures. Keep its original scope boundary.
    pub(super) fn record_pointer_check(
        &mut self,
        before: Var,
        bound: Vec<Var>,
        implied: Vec<(Var, Var)>,
        range: TextRange,
    ) {
        self.record_pointer_check_at(before, bound, implied, range, self.constraints.len());
    }

    pub(super) fn record_pointer_check_at(
        &mut self,
        before: Var,
        bound: Vec<Var>,
        input_wf: Vec<(Var, Var)>,
        range: TextRange,
        caller_constraints: usize,
    ) {
        let mut implied = input_wf.clone();
        // Resolve inferred data uppers using only the caller prefix preceding
        // this proof. Existing scopes keep their first expanded WF graph.
        for &(upper, hole) in &input_wf {
            if !self.universal_of.contains_key(&upper) {
                let mut pending = vec![upper];
                let mut seen = FxHashSet::default();
                while let Some(var) = pending.pop() {
                    if !seen.insert(var) {
                        continue;
                    }
                    if self.universal_of.contains_key(&var) {
                        implied.push((var, hole));
                    }
                    pending.extend(
                        self.constraints[..caller_constraints]
                            .iter()
                            .filter(|edge| edge.sub == var)
                            .map(|edge| edge.sup),
                    );
                    pending.extend(
                        self.known_pairs
                            .iter()
                            .filter(|&&(_, end)| end == var)
                            .map(|&(start, _)| start),
                    );
                }
            }
        }
        record_pointer_check(
            &mut self.pointer_checks,
            before,
            bound,
            input_wf,
            implied,
            range,
        );
    }

    /// A pointer's bound regions are fresh for every call. Its free regions
    /// and type captures belong to the pointer value and cannot be generalized.
    pub(super) fn pointer_inputs(
        &mut self,
        sig: flux_ty::FnSigTy,
        own: &[Var],
        input_types: &[Ty],
        inputs: &[Vec<Var>],
        range: TextRange,
    ) -> Option<Vec<Var>> {
        self.pointer_input_rows(sig, own, input_types, inputs, range, Variance::Covariant)
    }

    pub(super) fn pointer_equality_inputs(
        &mut self,
        sig: flux_ty::FnSigTy,
        own: &[Var],
        input_types: &[Ty],
        inputs: &[Vec<Var>],
        range: TextRange,
    ) -> Option<Vec<Var>> {
        self.pointer_input_rows(sig, own, input_types, inputs, range, Variance::Invariant)
    }

    fn pointer_input_rows(
        &mut self,
        sig: flux_ty::FnSigTy,
        own: &[Var],
        input_types: &[Ty],
        inputs: &[Vec<Var>],
        range: TextRange,
        variance: Variance,
    ) -> Option<Vec<Var>> {
        let count = match sig.regions {
            Some(r) => r.bound,
            None if sig.inputs_output.iter().all(|&ty| self.slots(ty) == 0) => 0,
            None => return None,
        };
        let before = self.vars;
        let mut placeholders = Vec::new();
        for &v in inputs.iter().flatten() {
            if self.holes.contains(&v) && !placeholders.contains(&v) {
                placeholders.push(v);
            }
        }
        if !placeholders.is_empty() {
            let implied = self
                .hole_pairs
                .iter()
                .copied()
                .filter(|&(a, b)| {
                    (placeholders.contains(&a) || !self.holes.contains(&a))
                        && (placeholders.contains(&b) || !self.holes.contains(&b))
                })
                .collect();
            self.record_pointer_check(before, placeholders, implied, range);
        }
        let bound = self.fresh_list(count);
        let rows = if sig.regions.is_some() {
            self.instantiate_pointer(sig, own, &bound)?
        } else {
            vec![Vec::new(); sig.inputs_output.len()]
        };
        if inputs.len() != sig.params().len() || input_types.len() != inputs.len() {
            return None;
        }
        for (((&ty, formal), actual), &source) in
            sig.params().iter().zip(&rows).zip(inputs).zip(input_types)
        {
            self.relate_types(
                source,
                actual,
                ty,
                formal,
                variance,
                Category::CallArgument,
                range,
            );
        }
        rows.last().cloned()
    }

    pub(super) fn pointer_call(
        &mut self,
        sig: flux_ty::FnSigTy,
        own: &[Var],
        input_types: &[Ty],
        inputs: &[Vec<Var>],
        dest: &Place,
        range: TextRange,
    ) {
        // C varargs have no declared region relationship with the result.
        // Only the fixed prefix participates in the pointer's signature proof.
        let count = if sig.is_variadic {
            sig.params().len()
        } else {
            inputs.len()
        };
        let Some(output) = input_types
            .get(..count)
            .zip(inputs.get(..count))
            .and_then(|(types, rows)| self.pointer_inputs(sig, own, types, rows, range))
        else {
            self.errors.push(Diagnostic::new(
                range,
                "function pointer region metadata is not supported yet",
            ));
            return;
        };
        let (target, result) = self.place_regions(dest);
        let category = self.category_into(dest);
        self.relate_types(
            sig.ret(),
            &output,
            target,
            &result,
            Variance::Covariant,
            category,
            range,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deferred_domains_keep_their_original_scope() {
        let mut checks = Vec::new();
        let first = TextRange::new(1, 2);
        let second = TextRange::new(3, 4);
        record_pointer_check(
            &mut checks,
            10,
            vec![2, 1, 2],
            vec![(3, 1)],
            vec![(3, 1)],
            first,
        );
        record_pointer_check(
            &mut checks,
            20,
            vec![1, 2],
            vec![(3, 1), (3, 1)],
            vec![(3, 1), (4, 1)],
            first,
        );
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].before, 10);
        assert_eq!(
            checks[0].implied,
            vec![(3, 1)],
            "later output-derived edges are not input assumptions"
        );
        record_pointer_check(
            &mut checks,
            9,
            vec![1, 2],
            vec![(3, 1)],
            vec![(3, 1)],
            first,
        );
        assert_eq!(checks[0].before, 9);
        record_pointer_check(
            &mut checks,
            20,
            vec![1, 2],
            vec![(3, 1)],
            vec![(3, 1)],
            second,
        );
        assert_eq!(
            checks.len(),
            1,
            "one binder can be observed at several locations"
        );
        assert_eq!(checks[0].before, 9);
        assert_eq!(checks[0].range, first);
        record_pointer_check(
            &mut checks,
            20,
            vec![1, 4],
            vec![(3, 1)],
            vec![(3, 1)],
            first,
        );
        record_pointer_check(
            &mut checks,
            20,
            vec![1, 2],
            vec![(3, 2)],
            vec![(3, 2)],
            first,
        );
        assert_eq!(checks.len(), 3);
        assert!(checks[1..].iter().all(|check| check.before == 20));
    }
    #[test]
    fn scoped_input_wf_keeps_inferred_data_nonrigid() {
        let (_, _, hir) = flux_hir::lower_text("main.rs", "fn main(){}");
        let typed = crate::check_file(&hir);
        assert!(typed.diagnostics.is_empty());
        let body = crate::Body::empty(None);
        let requirements = super::super::Requirements::default();
        for invalid in 0..3 {
            let mut cx = Cx::new(&body, &hir.items, &typed.sigs, &requirements);
            cx.new_universal("'static".into());
            let parent = cx.new_universal("'parent".into());
            let other = cx.new_universal("'other".into());
            let data = cx.fresh();
            let before = cx.vars;
            let universal = cx.universal.len();
            let known = cx.known_pairs.len();
            let input = cx.new_universal("'input".into());
            cx.close_known();
            // Input formation gives K:input. A later ordinary use fixes K to
            // the original caller lifetime; K itself remains inferred.
            cx.outlives(data, input, Category::CallArgument, TextRange::default());
            cx.equate(parent, data, Category::CallArgument, TextRange::default());
            if invalid == 1 {
                cx.outlives(input, STATIC, Category::CallArgument, TextRange::default());
            } else if invalid == 2 {
                cx.outlives(parent, other, Category::CallArgument, TextRange::default());
            }
            cx.finish_pointer_proof(
                before,
                &[input],
                &[(data, input)],
                universal,
                known,
                TextRange::default(),
            );
            assert_eq!(
                cx.errors.is_empty(),
                invalid == 0,
                "variant {invalid}: {:?}",
                cx.errors
            );
            assert!(!cx.universal_of.contains_key(&data));
        }
    }
    #[test]
    fn scoped_preflight_shares_closure_export_policy() {
        use super::super::{Ext, Requirements};
        let (_, _, hir) = flux_hir::lower_text("main.rs", "fn main(){}");
        let typed = crate::check_file(&hir);
        let parent = hir.items.functions.iter().last().unwrap().0;
        let requirements = Requirements::default();
        for annotated_refs in [false, true] {
            let mut body = crate::Body::empty(None);
            body.upvars = Some(crate::Upvars {
                kind: flux_ty::ClosureKind::FnMut,
                names: vec![],
                by_ref: vec![],
                mutable: vec![],
                annotated_refs,
                fixed_args: vec![1],
            });
            let mut cx = Cx::new(&body, &hir.items, &typed.sigs, &requirements);
            cx.closure_ext = vec![
                Some(Ext::Static),
                Some(Ext::SelfRef),
                Some(Ext::Arg(0)),
                Some(Ext::Arg(1)),
                Some(Ext::Parent(parent, 0)),
                None,
                Some(Ext::Ret(0)),
            ];
            assert!(cx.closure_relation(1, 0).is_none());
            assert_eq!(cx.closure_relation(2, 0).is_some(), !annotated_refs);
            assert!(cx.closure_relation(3, 0).is_some());
            assert!(cx.closure_relation(4, 3).is_some());
            assert!(cx.closure_relation(5, 0).is_none());
            assert!(cx.closure_relation(4, 5).is_none());
            assert_eq!(
                cx.closure_relation(4, 6),
                Some((Ext::Parent(parent, 0), Ext::Ret(0))),
            );
        }
    }
}
