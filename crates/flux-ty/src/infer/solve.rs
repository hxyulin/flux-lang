//! Traits in a body: the obligations calls create, selecting the `impl`
//! block or bound that meets each, and the methods and associated
//! functions found through traits.
//!
//! An obligation `ty: Tr` is met by a bound in scope (for generic
//! parameters) or by the one `impl` block of `Tr` whose header unifies
//! with it, whose own bounds become obligations in turn. While `ty` is an
//! unknown variable, or more than one block may apply, it waits; at the
//! end, integer variables take their default and what is left is
//! reported.

use std::collections::HashSet;

use flux_hir::{ExprId, FunctionId, ImplId, Owner, TraitId};
use flux_intern::Symbol;
use flux_syntax::TextRange;

use super::Ctx;
use crate::traits::{Predicate, TraitRef, owner_param_count, predicates_of};
use crate::ty::{IntTy, Ty, TyKind};

#[derive(Debug, Clone, Copy)]
pub(super) struct Obligation {
    pub pred: Predicate,
    /// The obligation it came from through `impl` blocks' bounds, which
    /// rustc names when it is not met.
    pub root: Predicate,
    pub range: TextRange,
    pub kind: ObligationKind,
}

/// How an unmet obligation is reported.
#[derive(Debug, Clone, Copy)]
pub(super) enum ObligationKind {
    /// "the trait bound `S: Tr` is not satisfied"
    Bound,
    /// A bound of the block an item was found in, on a type known when it
    /// was: "the method `m` exists for struct `W<S>`, but its trait
    /// bounds were not satisfied". `what` is "method" or "associated
    /// function or constant".
    ItemBounds {
        what: &'static str,
        name: Symbol,
        ty: Ty,
    },
    /// `Self: Tr` for `Tr::f()`, whose `Self` must become known; `range`
    /// is the call's, where rustc reports it if it does not.
    TraitCall { range: TextRange },
}

enum Selection {
    /// Met, if these are.
    Met(Vec<Predicate>),
    Ambiguous,
    Unmet,
}

/// Where a call is, for the spans of its obligations.
#[derive(Debug, Clone, Copy)]
pub(super) struct CallSite<'s> {
    /// The callee path, or the method's name.
    pub callee: TextRange,
    pub whole: TextRange,
    /// The receiver of a method call, the first argument.
    pub receiver: Option<ExprId>,
    pub args: &'s [ExprId],
    /// `<S as Tr>`'s `S`, where a failed `Self: Tr` is reported.
    pub qself: Option<TextRange>,
    /// The expression whose generic arguments were written.
    pub expr: ExprId,
}

impl Ctx<'_> {
    pub(super) fn register(&mut self, pred: Predicate, range: TextRange, kind: ObligationKind) {
        self.obligations.push(Obligation {
            pred,
            root: pred,
            range,
            kind,
        });
    }

    /// Whether `f` holds, leaving the variables as they were.
    pub(super) fn probe(&mut self, f: impl FnOnce(&mut Self) -> bool) -> bool {
        let snapshot = self.vars.clone();
        let result = f(self);
        self.vars = snapshot;
        result
    }

    /// Selects what can be, until nothing changes, reporting obligations
    /// that cannot be met.
    pub(super) fn select_obligations(&mut self) {
        loop {
            let pending = std::mem::take(&mut self.obligations);
            let mut progress = false;
            for ob in pending {
                match self.select_predicate(ob.pred) {
                    Selection::Met(nested) => {
                        progress = true;
                        let kind = match ob.kind {
                            ObligationKind::TraitCall { .. } => ObligationKind::Bound,
                            kind => kind,
                        };
                        for pred in nested {
                            self.obligations.push(Obligation { pred, kind, ..ob });
                        }
                    }
                    Selection::Ambiguous => self.obligations.push(ob),
                    Selection::Unmet => {
                        progress = true;
                        self.report_unmet(ob);
                    }
                }
            }
            if !progress {
                break;
            }
        }
    }

    /// Once the body is inferred: integer variables in waiting
    /// obligations take their default, and what still waits is
    /// reported.
    pub(super) fn finish_obligations(&mut self, reported: &mut HashSet<crate::ty::InferVar>) {
        self.select_obligations();
        let pending = self.obligations.clone();
        for ob in &pending {
            let mut vars = Vec::new();
            self.int_vars(ob.pred.self_ty, &mut vars);
            for var in vars {
                self.vars[var.0 as usize].0 = super::VarValue::Known(Ty::int(IntTy::I32));
            }
        }
        self.select_obligations();
        // As in rustc, a body with errors reports no ambiguity: what is
        // unknown is likely so because of them.
        let tainted = !self.diagnostics.is_empty();
        for ob in std::mem::take(&mut self.obligations) {
            if tainted {
                continue;
            }
            let self_ty = self.shallow(ob.pred.self_ty);
            let mut unknown = Vec::new();
            self.unknown_vars(self_ty, &mut unknown);
            if unknown.iter().any(|v| reported.contains(v)) {
                continue;
            }
            reported.extend(unknown.iter().copied());
            match ob.kind {
                ObligationKind::TraitCall { range }
                    if matches!(self_ty.kind(), TyKind::Infer(_)) =>
                {
                    self.error(
                        range,
                        "cannot call associated function on trait without specifying the corresponding `impl` type",
                    );
                }
                _ => self.error(ob.range, "type annotations needed"),
            }
        }
    }

    fn int_vars(&mut self, ty: Ty, out: &mut Vec<crate::ty::InferVar>) {
        let ty = self.shallow(ty);
        match ty.kind() {
            TyKind::Infer(var) if self.unknown(var).0 => out.push(self.root(var)),
            TyKind::Tuple(list) | TyKind::Adt(_, list) => {
                for &t in list {
                    self.int_vars(t, out);
                }
            }
            TyKind::Ref(_, t) | TyKind::RawPtr(_, t) => self.int_vars(t, out),
            _ => {}
        }
    }

    fn report_unmet(&mut self, ob: Obligation) {
        let sized =
            self.items.traits[ob.root.trait_ref.trait_].lang == Some(flux_intern::sym::sized);
        let message = match ob.kind {
            _ if sized => format!(
                "the size for values of type `{}` cannot be known at compilation time",
                self.describe(ob.root.self_ty)
            ),
            ObligationKind::ItemBounds { what, name, ty } => {
                format!(
                    "the {what} `{name}` exists for {}, but its trait bounds were not satisfied",
                    self.describe_kind(ty)
                )
            }
            _ => {
                let self_ty = self.describe(ob.root.self_ty);
                let args: Vec<String> = ob
                    .root
                    .trait_ref
                    .args
                    .iter()
                    .map(|&a| self.describe(a))
                    .collect();
                let args = if args.is_empty() {
                    String::new()
                } else {
                    format!("<{}>", args.join(", "))
                };
                format!(
                    "the trait bound `{self_ty}: {}{args}` is not satisfied",
                    self.trait_path(ob.root.trait_ref.trait_)
                )
            }
        };
        if self.reported_unmet.insert((ob.range, message.clone())) {
            self.error(ob.range, message);
        }
    }

    /// A trait as rustc names it in messages: by its path from the crate
    /// root (`m::Tr`) if it is the program's, else by its name.
    fn trait_path(&self, t: TraitId) -> String {
        let items = self.items;
        let data = &items.traits[t];
        let module = data.loc.module;
        if !items.in_main_crate(module) || items.modules[module].parent.is_none() {
            return data.name.to_string();
        }
        let path = items.module_path(module);
        let path = path.strip_prefix("crate::").unwrap_or(&path);
        format!("{path}::{}", data.name)
    }

    /// "struct `S`", "reference `&S`", "type parameter `T`", ...: a type
    /// as rustc's method errors name it.
    pub(super) fn describe_kind(&mut self, ty: Ty) -> String {
        let kind = match self.shallow(ty).kind() {
            TyKind::Adt(..) => "struct",
            TyKind::Ref(..) => "reference",
            TyKind::Tuple(_) => "tuple",
            TyKind::Param(_) => "type parameter",
            TyKind::RawPtr(..) => "raw pointer",
            _ => "type",
        };
        format!("{kind} `{}`", self.describe(ty))
    }

    fn select_predicate(&mut self, pred: Predicate) -> Selection {
        let self_ty = self.shallow(pred.self_ty);
        if self_ty == Ty::ERROR || pred.trait_ref.args.contains(&Ty::ERROR) {
            return Selection::Met(Vec::new());
        }
        if let TyKind::Infer(var) = self_ty.kind()
            && !self.unknown(var).0
        {
            return Selection::Ambiguous;
        }
        let t = pred.trait_ref.trait_;
        // Every type flux has is sized, except perhaps a trait's `Self`,
        // which is only if a bound says so.
        if self.items.traits[t].lang == Some(flux_intern::sym::sized)
            && self_ty != Ty::param(0, flux_intern::kw::SelfUpper)
        {
            return Selection::Met(Vec::new());
        }
        let env: Vec<Predicate> = self
            .param_env
            .iter()
            .filter(|p| p.trait_ref.trait_ == t)
            .copied()
            .collect();
        let bounds: Vec<Predicate> = env
            .into_iter()
            .filter(|&p| self.probe(|cx| cx.unify_predicates(p, pred)))
            .collect();
        match bounds.as_slice() {
            [bound] => {
                self.unify_predicates(*bound, pred);
                return Selection::Met(Vec::new());
            }
            [] => {}
            _ => return Selection::Ambiguous,
        }
        let impls = self.items.trait_impls.get(&t).map_or(&[][..], |v| v);
        let found: Vec<ImplId> = impls
            .iter()
            .copied()
            .filter(|&imp| self.probe(|cx| cx.unify_impl(imp, pred).is_some()))
            .collect();
        // Of several blocks, those whose bounds cannot hold are no
        // candidates, as rustc's winnowing has it.
        let found = if found.len() > 1 {
            found
                .into_iter()
                .filter(|&imp| {
                    self.probe(|cx| {
                        let Some(args) = cx.unify_impl(imp, pred) else {
                            return false;
                        };
                        let nested = cx.sigs.impl_predicates.get(imp).map_or(&[][..], |p| p);
                        nested.iter().all(|p| {
                            let p = p.subst(&args);
                            let args: Vec<Ty> = p.trait_ref.args.to_vec();
                            cx.may_hold(p.self_ty, p.trait_ref.trait_, Some(&args), 1)
                        })
                    })
                })
                .collect()
        } else {
            found
        };
        match found.as_slice() {
            [] => Selection::Unmet,
            &[imp] => {
                let args = self.unify_impl(imp, pred).expect("probed");
                let nested = self.sigs.impl_predicates.get(imp).map_or(&[][..], |p| p);
                Selection::Met(nested.iter().map(|p| p.subst(&args)).collect())
            }
            _ => Selection::Ambiguous,
        }
    }

    fn unify_predicates(&mut self, a: Predicate, b: Predicate) -> bool {
        if a.trait_ref.trait_ != b.trait_ref.trait_ {
            return false;
        }
        let mut ok = self.unify(a.self_ty, b.self_ty);
        for (&x, &y) in a.trait_ref.args.iter().zip(b.trait_ref.args) {
            ok &= self.unify(x, y);
        }
        ok
    }

    /// Unifies `impl` block `imp`'s header, with new variables for its
    /// parameters, with `pred`: those variables, if it unifies.
    fn unify_impl(&mut self, imp: ImplId, pred: Predicate) -> Option<Vec<Ty>> {
        let (Some(&self_ty), Some(&trait_ref)) =
            (self.sigs.impls.get(imp), self.sigs.impl_traits.get(imp))
        else {
            return None;
        };
        let count = self.items.impls[imp].generics.len();
        let vars: Vec<Ty> = (0..count)
            .map(|_| self.new_var(false, TextRange::default()))
            .collect();
        let header = trait_ref.subst(&vars).with_self(self_ty.subst(&vars));
        self.unify_predicates(header, pred).then_some(vars)
    }

    /// Whether `ty` may implement `t`: through a bound in scope, or an
    /// `impl` block of `t` that may be for it and whose bounds may hold.
    pub(super) fn may_implement(&mut self, ty: Ty, t: TraitId) -> bool {
        self.probe(|cx| cx.may_hold(ty, t, None, 0))
    }

    /// Whether an `impl` block of `t` may be for `ty`, its bounds aside:
    /// a method found through it "exists" for rustc's messages.
    fn has_impl_header(&mut self, ty: Ty, t: TraitId) -> bool {
        self.probe(|cx| cx.may_hold(ty, t, None, u32::MAX - 1))
    }

    /// Whether a trait item called `name` exists for `ty` through an
    /// `impl` block whose bounds do not hold: rustc's "the method `m`
    /// exists for struct `W<S>`, but its trait bounds were not
    /// satisfied".
    pub(super) fn exists_unmet(&mut self, ty: Ty, name: Symbol, methods: bool) -> bool {
        let items = self.items;
        if !items.trait_fns.contains_key(&name) {
            return false;
        }
        let in_scope = self.traits_in_scope().to_vec();
        in_scope.into_iter().any(|t| {
            items.traits[t]
                .function(items, name)
                .is_some_and(|f| !methods || items.functions[f].has_self)
                && self.has_impl_header(ty, t)
        })
    }

    /// Whether `ty: t` (with `args`, if given) may hold, unifying as it
    /// goes; call it in a probe.
    fn may_hold(&mut self, ty: Ty, t: TraitId, args: Option<&[Ty]>, depth: u32) -> bool {
        let ty = self.shallow(ty);
        if ty == Ty::ERROR || matches!(ty.kind(), TyKind::Infer(v) if !self.unknown(v).0) {
            return true;
        }
        // Deep enough, or (from `has_impl_header`) bounds aside.
        let check_bounds = depth < 8;
        if self.items.traits[t].lang == Some(flux_intern::sym::sized) {
            return true;
        }
        let in_env = self.param_env.iter().any(|p| {
            p.self_ty == ty && p.trait_ref.trait_ == t && args.is_none_or(|a| a == p.trait_ref.args)
        });
        if in_env {
            return true;
        }
        let impls = self.items.trait_impls.get(&t).map_or(&[][..], |v| v);
        impls.iter().any(|&imp| {
            self.probe(|cx| {
                let (Some(&self_ty), Some(&trait_ref)) =
                    (cx.sigs.impls.get(imp), cx.sigs.impl_traits.get(imp))
                else {
                    return false;
                };
                let count = cx.items.impls[imp].generics.len();
                let vars: Vec<Ty> = (0..count)
                    .map(|_| cx.new_var(false, TextRange::default()))
                    .collect();
                if !cx.unify(self_ty.subst(&vars), ty) {
                    return false;
                }
                if let Some(args) = args {
                    let header = trait_ref.subst(&vars);
                    for (&x, &y) in header.args.iter().zip(args) {
                        if !cx.unify(x, y) {
                            return false;
                        }
                    }
                }
                if !check_bounds {
                    return true;
                }
                let nested = cx.sigs.impl_predicates.get(imp).map_or(&[][..], |p| p);
                nested.iter().all(|p| {
                    let p = p.subst(&vars);
                    let args: Vec<Ty> = p.trait_ref.args.to_vec();
                    cx.may_hold(p.self_ty, p.trait_ref.trait_, Some(&args), depth + 1)
                })
            })
        })
    }

    /// The traits whose methods may be called here: those named in the
    /// module's scope, and the prelude's.
    pub(super) fn traits_in_scope(&mut self) -> &[TraitId] {
        if self.traits_in_scope.is_none() {
            let items = self.items;
            let mut found = Vec::new();
            let scopes = [Some(self.module), items.def_map.prelude];
            for module in scopes.into_iter().flatten() {
                for binding in items.def_map.scopes[module.index()].types.values() {
                    if let flux_hir::def_map::Def::Trait(t) = binding.def
                        && !found.contains(&t)
                    {
                        found.push(t);
                    }
                }
            }
            found.sort_by_key(|t| t.index());
            self.traits_in_scope = Some(found);
        }
        self.traits_in_scope.as_deref().unwrap_or_default()
    }

    /// The trait items called `name` that may be called on `ty`: from
    /// the bounds in scope on it, and from the traits in scope that it
    /// may implement; with the trait's generic arguments when a bound
    /// gives them. `methods` for those with `self` only.
    pub(super) fn trait_items(
        &mut self,
        ty: Ty,
        name: Symbol,
        methods: bool,
    ) -> Vec<(FunctionId, Option<TraitRef>)> {
        let ty = self.shallow(ty);
        let items = self.items;
        if !items.trait_fns.contains_key(&name) {
            return Vec::new();
        }
        let wanted = |t: TraitId| {
            items.traits[t]
                .function(items, name)
                .filter(|&f| !methods || items.functions[f].has_self)
        };
        let mut found: Vec<(FunctionId, Option<TraitRef>)> = Vec::new();
        if ty.has_params() {
            for p in self.param_env.clone() {
                if p.self_ty == ty
                    && let Some(f) = wanted(p.trait_ref.trait_)
                    && !found.iter().any(|(g, _)| *g == f)
                {
                    found.push((f, Some(p.trait_ref)));
                }
            }
        }
        if let TyKind::Infer(var) = ty.kind()
            && !self.unknown(var).0
        {
            return found;
        }
        let in_scope = self.traits_in_scope().to_vec();
        for t in in_scope {
            let Some(f) = wanted(t) else {
                continue;
            };
            if found.iter().any(|(g, _)| *g == f) {
                continue;
            }
            if self.may_implement(ty, t) {
                found.push((f, None));
            }
        }
        found
    }

    /// The function an associated path `callee` names: `T::f`, `S::f`,
    /// `<T as Tr>::f`, `<T>::f` or `Tr::f`, with the type it is called on
    /// (`Self`, for a trait's) and the trait's generic arguments, if
    /// known. `None` if there is none (reported).
    pub(super) fn resolve_assoc_fn(
        &mut self,
        callee: ExprId,
        ty: Option<flux_hir::TypeRefId>,
        trait_: Option<TraitId>,
        name: Symbol,
    ) -> Option<(FunctionId, Option<Ty>, Option<Vec<Ty>>)> {
        let segment = self.source_map.segments.get(callee).copied();
        let segment = segment.unwrap_or_else(|| self.range(callee));
        let self_ty = ty.map(|t| self.resolve_path_type(t));
        if self_ty == Some(Ty::ERROR) {
            return None;
        }
        if let Some(t) = trait_ {
            let items = self.items;
            let function = items.traits[t].function(items, name)?;
            let written = self
                .body
                .generic_args
                .get(callee)
                .and_then(|g| g.ty.clone());
            let trait_args = match written {
                Some(written) => {
                    let want = items.traits[t].generics.len();
                    let args: Vec<Ty> = written
                        .types
                        .iter()
                        .map(|&ty| self.resolve_type_ref(ty))
                        .collect();
                    if args.len() != want {
                        if !args.contains(&Ty::ERROR) {
                            let message = crate::wrong_arg_count("trait", want, args.len());
                            self.error(written.segment, message);
                        }
                        return None;
                    }
                    Some(args)
                }
                None => None,
            };
            return Some((function, self_ty, trait_args));
        }
        let self_ty = self.shallow(self_ty?);
        let found = self.trait_items(self_ty, name, false);
        match found.as_slice() {
            [(function, trait_ref)] => {
                Some((*function, Some(self_ty), trait_ref.map(|t| t.args.to_vec())))
            }
            [] => {
                let items = self.items;
                // An unsupported item of a trait (reported).
                if items.traits.values().any(|t| t.unsupported.contains(&name)) {
                    return None;
                }
                let in_some_trait = items
                    .traits
                    .values()
                    .any(|t| t.function(items, name).is_some());
                if self.exists_unmet(self_ty, name, false) {
                    let message = format!(
                        "the associated function or constant `{name}` exists for {}, but its trait bounds were not satisfied",
                        self.describe_kind(self_ty)
                    );
                    self.error(segment, message);
                    return None;
                }
                let message = match self_ty.kind() {
                    TyKind::Int(_) | TyKind::Bool if !in_some_trait => {
                        format!("`{}::{name}` is not supported yet", self.describe(self_ty))
                    }
                    _ => format!(
                        "no associated function or constant named `{name}` found for {} in the current scope",
                        self.describe_kind(self_ty)
                    ),
                };
                self.error(segment, message);
                None
            }
            _ => {
                self.error(segment, "multiple applicable items in scope");
                None
            }
        }
    }

    /// In a trait's provided method, `Self` may be unsized unless a bound
    /// says otherwise, so no parameter or local may have it as its type.
    pub(super) fn check_unsized_self(&mut self) {
        let self_ty = Ty::param(0, flux_intern::kw::SelfUpper);
        let sized = self.param_env.iter().any(|p| {
            p.self_ty == self_ty
                && self.items.traits[p.trait_ref.trait_].lang == Some(flux_intern::sym::sized)
        });
        if sized {
            return;
        }
        let body = self.body;
        let message = "the size for values of type `Self` cannot be known at compilation time";
        // Calls that return `Self`, and the bindings that hold it.
        let mut ranges = Vec::new();
        for (expr, data) in body.exprs.iter() {
            if matches!(
                data,
                flux_hir::Expr::Call { .. } | flux_hir::Expr::MethodCall { .. }
            ) && let Some(&ty) = self.expr_types.get(expr)
                && self.shallow(ty) == self_ty
            {
                ranges.push(self.range(expr));
            }
        }
        for (pat, data) in body.pats.iter() {
            let flux_hir::Pat::Bind(binding) = *data else {
                continue;
            };
            let ty = self
                .binding_types
                .get(binding)
                .copied()
                .unwrap_or(Ty::ERROR);
            if self.shallow(ty) == self_ty {
                ranges.push(self.source_map.pats.get(pat).copied().unwrap_or_default());
            }
        }
        for range in ranges {
            self.error(range, message);
        }
    }

    /// The type a path names before an associated item: as a type, except
    /// that a generic struct without arguments (`W::f`) gets new
    /// variables for them, as in an expression.
    fn resolve_path_type(&mut self, ty: flux_hir::TypeRefId) -> Ty {
        let items = self.items;
        if let flux_hir::TypeRef::Path(path) = &self.body.types[ty]
            && path.qself.is_none()
            && path.args(path.segments.len().saturating_sub(1)).is_none()
            && let Ok(flux_hir::def_map::Resolved::Def(flux_hir::def_map::Def::Struct(s))) =
                items.resolve(self.module, path, flux_hir::def_map::Ns::Types)
            && !items.structs[s].generics.is_empty()
        {
            let origin = self.source_map.types.get(ty).copied().unwrap_or_default();
            let count = items.structs[s].generics.len();
            let args: Vec<Ty> = (0..count).map(|_| self.new_var(false, origin)).collect();
            return Ty::adt(s, items.structs[s].name, &args);
        }
        self.resolve_type_ref(ty)
    }

    /// Whether a block for another instance of `ty`'s struct has a method
    /// `name`: rustc then names the struct with its parameters (`W<T>`)
    /// in "no method named" errors.
    pub(super) fn other_instance_has(&mut self, ty: Ty, name: Symbol) -> bool {
        let TyKind::Adt(adt, args) = ty.kind() else {
            return false;
        };
        let items = self.items;
        if args.is_empty() {
            return false;
        }
        let target = flux_hir::ImplTarget::Struct(adt.strukt);
        if items.assoc.contains_key(&(target, name)) {
            return true;
        }
        let traits = items.trait_fns.get(&name).map_or(&[][..], |v| v);
        traits.iter().any(|t| {
            let impls = items.trait_impls.get(t).map_or(&[][..], |v| v);
            impls.iter().any(|&imp| {
                matches!(self.sigs.impls.get(imp).map(|t| t.kind()), Some(TyKind::Adt(a, _)) if a.strukt == adt.strukt)
            })
        })
    }

    /// Registers the bounds of a call of `function` with generic
    /// arguments `args`, at `site`; `target`, the type the item was found
    /// on if it was known then, makes its block's bounds rustc's "exists
    /// but its trait bounds were not satisfied".
    pub(super) fn register_call_bounds(
        &mut self,
        function: FunctionId,
        args: &[Ty],
        site: CallSite<'_>,
        target: Option<Ty>,
    ) {
        let (items, sigs) = (self.items, self.sigs);
        let data = &items.functions[function];
        let own = sigs.fn_predicates.get(function).map_or(0, |p| p.len());
        let owner_bounds = match data.owner {
            Some(Owner::Impl(imp)) => sigs.impl_predicates.get(imp).is_some_and(|p| !p.is_empty()),
            Some(Owner::Trait(_)) => true,
            None => false,
        };
        if own == 0 && !owner_bounds {
            return;
        }
        let preds = predicates_of(items, sigs, function);
        let owner_count = match data.owner {
            Some(Owner::Impl(imp)) => sigs.impl_predicates.get(imp).map_or(0, |p| p.len()),
            Some(Owner::Trait(t)) => 1 + sigs.trait_predicates.get(t).map_or(0, |p| p.len()),
            None => 0,
        };
        let what = if site.receiver.is_some() {
            "method"
        } else {
            "associated function or constant"
        };
        for (k, p) in preds.iter().enumerate() {
            let pred = p.subst(args);
            let known = target.filter(|&t| !self.has_unknown(t));
            let (range, kind) = match (data.owner, known) {
                (Some(Owner::Trait(_)), _) if k == 0 => match (site.qself, known) {
                    (Some(qself), _) => (qself, ObligationKind::Bound),
                    (None, Some(ty)) => (
                        site.callee,
                        ObligationKind::ItemBounds {
                            what,
                            name: data.name,
                            ty,
                        },
                    ),
                    (None, None) if site.receiver.is_some() => (site.callee, ObligationKind::Bound),
                    (None, None) => (
                        self.bound_range(function, *p, site),
                        ObligationKind::TraitCall { range: site.whole },
                    ),
                },
                (Some(Owner::Impl(_)), Some(ty)) if k < owner_count => (
                    site.callee,
                    ObligationKind::ItemBounds {
                        what,
                        name: data.name,
                        ty,
                    },
                ),
                _ => (self.bound_range(function, *p, site), ObligationKind::Bound),
            };
            self.register(pred, range, kind);
        }
        self.select_obligations();
    }

    /// Whether `ty` has variables nothing gave a type yet.
    fn has_unknown(&mut self, ty: Ty) -> bool {
        let mut unknown = Vec::new();
        self.unknown_vars(ty, &mut unknown);
        self.int_vars(ty, &mut unknown);
        !unknown.is_empty()
    }

    /// Where rustc reports bound `p` of a call of `function` unmet: at
    /// the generic argument written for the parameter it bounds, else at
    /// the one argument whose type mentions it, else at the callee if
    /// several do, else at the whole call.
    fn bound_range(&self, function: FunctionId, p: Predicate, site: CallSite<'_>) -> TextRange {
        let (items, sigs) = (self.items, self.sigs);
        let data = &items.functions[function];
        let Some(param) = first_param(p.self_ty) else {
            return site.callee;
        };
        let first_own = owner_param_count(items, data.owner) as u32;
        if param < first_own {
            // A bound on the block's or trait's parameter in the
            // method's `where`.
            let own = sigs.fn_predicates.get(function).map_or(&[][..], |p| p);
            if own.contains(&p) {
                return site.callee;
            }
        }
        if param >= first_own
            && let Some(written) = self
                .body
                .generic_args
                .get(site.expr)
                .and_then(|g| g.item.as_ref())
            && let Some(&ty) = written.types.get((param - first_own) as usize)
            && let Some(&range) = self.source_map.types.get(ty)
        {
            return range;
        }
        let sig = &sigs[function];
        let mentions: Vec<usize> = sig
            .params
            .iter()
            .enumerate()
            .filter(|(_, ty)| mentions(**ty, param))
            .map(|(i, _)| i)
            .collect();
        match mentions.as_slice() {
            [] => site.whole,
            &[i] => {
                let expr = match site.receiver {
                    Some(receiver) if i == 0 => Some(receiver),
                    Some(_) => site.args.get(i - 1).copied(),
                    None => site.args.get(i).copied(),
                };
                expr.map_or(site.callee, |e| self.range(e))
            }
            _ => site.callee,
        }
    }
}

/// The first generic parameter `ty` mentions.
fn first_param(ty: Ty) -> Option<u32> {
    match ty.kind() {
        TyKind::Param(p) => Some(p.index),
        TyKind::Ref(_, t) | TyKind::RawPtr(_, t) => first_param(t),
        TyKind::Tuple(list) | TyKind::Adt(_, list) => list.iter().find_map(|&t| first_param(t)),
        _ => None,
    }
}

fn mentions(ty: Ty, index: u32) -> bool {
    crate::mentions_param(ty, index)
}
