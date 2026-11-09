//! Scoped function-pointer binder proofs and per-call instantiation.
use super::{Category, Cx, FunctionUse, STATIC, UniversalRelationMode, Var};
use crate::Place;
use flux_hir::Diagnostic;
use flux_syntax::TextRange;
use flux_ty::regions::Variance;
use flux_ty::{Ty, TyKind};
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
    if let Some(old) = checks
        .iter_mut()
        .find(|old| old.bound == bound && old.input_wf == input_wf && old.range == range)
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

impl Cx<'_> {
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
            let upper: Vec<_> = implied
                .iter()
                .filter_map(|&(a, b)| (a < before && allowed(b, placeholder)).then_some(a))
                .collect();
            for old in 1..before {
                if above[old as usize] && !allowed(old, placeholder) {
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
        for var in bound {
            self.holes.remove(var);
        }
    }

    pub(super) fn pointer_proof_inputs(
        &mut self,
        target: flux_ty::FnSigTy,
        own: &[Var],
        bound: &[Var],
    ) -> Option<Vec<Vec<Var>>> {
        let rows =
            flux_ty::regions::instantiate_fn_pointer(self.items, target, own, bound, STATIC)?;
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

    pub(super) fn pointer_from_item(
        &mut self,
        function: flux_hir::FunctionId,
        args: &[Ty],
        target: flux_ty::FnSigTy,
        own: &[Var],
        written: Option<&flux_ty::regions::TypeArgRegions>,
        range: TextRange,
    ) {
        let Some(r) = target.regions else {
            if (written.is_none()
                && args
                    .iter()
                    .any(|&ty| flux_ty::regions::slots(self.items, ty) != 0))
                || self
                    .item_instantiation(function, args, written, FunctionUse::Reification, range)
                    .is_none()
            {
                self.errors.push(Diagnostic::new(
                    range,
                    "function item pointer region provenance is not supported yet",
                ));
            }
            return;
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
                // An early-bound owner or a type argument carrying regions
                // needs item-value provenance, not erased type equality.
                // A fixed pointer signature can infer owner lifetimes through
                // the same checked owner obligations as a direct call. A
                // universal pointer binder still needs independent provenance.
                if (regions.owner == 0 || r.bound == 0)
                    && (written.is_some()
                        || !args
                            .iter()
                            .any(|&t| flux_ty::regions::slots(self.items, t) != 0))
                    && !(r.bound != 0
                        && regions.callback_binder != flux_ty::regions::CallbackBinder::Independent)
                {
                    let signature = self.sigs.fns[function].subst(args);
                    let data = &self.items.functions[function];
                    let sig = flux_ty::FnSigTy {
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
            flux_ty::regions::instantiate_fn_pointer(self.items, sig, own, &bound, STATIC)?
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
        let Some(output) = self.pointer_inputs(sig, own, input_types, inputs, range) else {
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
        assert_eq!(checks.len(), 4);
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
                Ext::Static,
                Ext::SelfRef,
                Ext::Arg(0),
                Ext::Arg(1),
                Ext::Parent(parent, 0),
            ];
            assert!(!cx.closure_relation_exportable(1, 0));
            assert_eq!(cx.closure_relation_exportable(2, 0), !annotated_refs);
            assert!(cx.closure_relation_exportable(3, 0));
            assert!(cx.closure_relation_exportable(4, 3));
            assert!(!cx.closure_relation_exportable(5, 0));
            assert!(!cx.closure_relation_exportable(4, 5));
        }
    }
}
