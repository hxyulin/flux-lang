//! Scoped function-pointer binder proofs and per-call instantiation.
use super::{Category, Cx, FunctionUse, STATIC, Var};
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
    pub range: TextRange,
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
        let mut invalid = !self.caller_equalities_fit(&[]);
        let mut promote = Vec::new();
        for &placeholder in bound {
            let below = reachable(placeholder, &edges);
            let above = reachable(placeholder, &reverse);
            if bound.iter().any(|&other| {
                other != placeholder && below[other as usize] && !allowed(placeholder, other)
            }) {
                invalid = true;
            }
            if (1..before).any(|old| below[old as usize] && !allowed(placeholder, old)) {
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
        if source.is_c != target.is_c
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
        range: TextRange,
    ) {
        let Some(r) = target.regions else {
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
                    self.closure_goal(id, Some(&[]), Some(&closure_inputs), Some(&result), range);
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
                if regions.owner == 0
                    && !args
                        .iter()
                        .any(|&t| flux_ty::regions::slots(self.items, t) != 0)
                    && !(r.bound != 0
                        && regions.callback_binder != flux_ty::regions::CallbackBinder::Independent)
                {
                    let signature = self.sigs.fns[function].subst(args);
                    let data = &self.items.functions[function];
                    let sig = flux_ty::FnSigTy {
                        is_c: data.c_abi,
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
            self.pointer_checks.push(PointerCheck {
                before,
                bound: placeholders,
                implied,
                range,
            });
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
                Variance::Covariant,
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
