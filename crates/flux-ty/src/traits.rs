//! Traits outside of bodies: the bounds of items as predicates, the
//! supertraits they imply, whether a type implements a trait given the
//! bounds in scope, and which function a call of a trait's method calls
//! once its types are known.
//!
//! Inside a body, where types have variables, inference selects `impl`
//! blocks itself ([`crate::infer`]); here every type is known, up to the
//! generic parameters in scope, which are treated as distinct types that
//! implement what their bounds say.

use std::fmt;

use flux_hir::{
    Bound, Diagnostic, FunctionId, Generics, ImplId, ItemTree, ModuleId, Owner, TraitId,
};
use flux_intern::{Symbol, kw};
use flux_syntax::TextRange;

use crate::ty::{GenericArgs, Ty, TyKind};
use crate::{Instance, Signatures, TypeScope, resolve_type, wrong_arg_count};

/// A trait with generic arguments for its own parameters, not `Self`:
/// `Tr<A>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TraitRef {
    pub trait_: TraitId,
    pub name: Symbol,
    pub args: GenericArgs,
}

/// `ty: Tr<A>`: that a type implements a trait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Predicate {
    pub self_ty: Ty,
    pub trait_ref: TraitRef,
}

impl TraitRef {
    pub fn subst(self, args: &[Ty]) -> TraitRef {
        TraitRef {
            args: Ty::list(&crate::subst_list(self.args, args)),
            ..self
        }
    }

    /// The predicate `self_ty: self`.
    pub fn with_self(self, self_ty: Ty) -> Predicate {
        Predicate {
            self_ty,
            trait_ref: self,
        }
    }
}

impl Predicate {
    pub fn subst(self, args: &[Ty]) -> Predicate {
        Predicate {
            self_ty: self.self_ty.subst(args),
            trait_ref: self.trait_ref.subst(args),
        }
    }
}

impl fmt::Display for TraitRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        if !self.args.is_empty() {
            let args: Vec<String> = self.args.iter().map(Ty::to_string).collect();
            write!(f, "<{}>", args.join(", "))?;
        }
        Ok(())
    }
}

impl fmt::Display for Predicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.self_ty, self.trait_ref)
    }
}

/// A trait's resolved header.
#[derive(Debug, Clone, Default)]
pub struct TraitSig {
    /// `Sup` in `trait Tr: Sup`, in terms of the trait's parameters,
    /// `Self` first.
    pub supertraits: Box<[TraitRef]>,
}

/// The trait bound `bound`, written in `module`: its trait, with the
/// generic arguments written after its last segment, one for each of the
/// trait's parameters. `None` if it did not resolve (reported in
/// `errors`).
pub fn resolve_bound(
    items: &ItemTree,
    module: ModuleId,
    bound: &Bound,
    scope: TypeScope<'_>,
    errors: &mut Vec<(TextRange, String)>,
) -> Option<TraitRef> {
    let (t, error) = items.resolve_trait(module, bound);
    if let Some(error) = error {
        errors.push(error);
    }
    trait_ref(items, module, t?, bound, scope, errors)
}

/// Trait `t`, named by `bound`, with the generic arguments written after
/// its last segment.
pub fn trait_ref(
    items: &ItemTree,
    module: ModuleId,
    t: TraitId,
    bound: &Bound,
    scope: TypeScope<'_>,
    errors: &mut Vec<(TextRange, String)>,
) -> Option<TraitRef> {
    let data = &items.traits[t];
    let path = &bound.path;
    let last = path.segments.len().saturating_sub(1);
    let at = path.range(last, bound.range);
    let want = data.generics.len();
    let args: Vec<Ty> = match path.args(last) {
        Some(written) => written
            .types
            .iter()
            .zip(&written.ranges)
            .map(|(ty, &range)| resolve_type(items, module, ty, scope, range, errors))
            .collect(),
        None if want == 0 => Vec::new(),
        None => {
            errors.push((at, format!("missing generics for trait `{}`", data.name)));
            return None;
        }
    };
    if args.len() != want {
        errors.push((at, wrong_arg_count("trait", want, args.len())));
        return None;
    }
    if args.contains(&Ty::ERROR) {
        return None;
    }
    Some(TraitRef {
        trait_: t,
        name: data.name,
        args: Ty::list(&args),
    })
}

/// The predicates of `generics`, the bounds in its list and `where`
/// clause, written in `module`; those that did not resolve are left out
/// (reported in `errors`).
pub fn resolve_predicates(
    items: &ItemTree,
    module: ModuleId,
    generics: &Generics,
    scope: TypeScope<'_>,
    errors: &mut Vec<(TextRange, String)>,
) -> Vec<Predicate> {
    let mut out = Vec::new();
    for p in generics.predicates.iter() {
        let self_ty = resolve_type(items, module, &p.ty, scope, p.ty_range, errors);
        let Some(trait_ref) = resolve_bound(items, module, &p.bound, scope, errors) else {
            continue;
        };
        if self_ty != Ty::ERROR {
            out.push(trait_ref.with_self(self_ty));
        }
    }
    out
}

/// How many generic parameters come before an associated item's own:
/// its `impl` block's, or its trait's and `Self`.
pub fn owner_param_count(items: &ItemTree, owner: Option<Owner>) -> usize {
    match owner {
        Some(Owner::Impl(imp)) => items.impls[imp].generics.len(),
        Some(Owner::Trait(t)) => 1 + items.traits[t].generics.len(),
        None => 0,
    }
}

/// `Self: Tr<P..>` for trait `t`, in terms of its own parameters.
pub fn trait_self_predicate(items: &ItemTree, t: TraitId) -> Predicate {
    let data = &items.traits[t];
    let args = crate::params_of(&data.generics, 1);
    Predicate {
        self_ty: Ty::param(0, kw::SelfUpper),
        trait_ref: TraitRef {
            trait_: t,
            name: data.name,
            args: Ty::list(&args),
        },
    }
}

/// The bounds a function's body may rely on: its owner's (`Self: Tr` in
/// a trait), then its own, in terms of all its parameters, not yet
/// elaborated with supertraits.
pub fn predicates_of(items: &ItemTree, sigs: &Signatures, function: FunctionId) -> Vec<Predicate> {
    let mut out = Vec::new();
    match items.functions[function].owner {
        Some(Owner::Impl(imp)) => {
            out.extend(sigs.impl_predicates.get(imp).into_iter().flatten().copied());
        }
        Some(Owner::Trait(t)) => {
            out.push(trait_self_predicate(items, t));
            out.extend(sigs.trait_predicates.get(t).into_iter().flatten().copied());
        }
        None => {}
    }
    out.extend(
        sigs.fn_predicates
            .get(function)
            .into_iter()
            .flatten()
            .copied(),
    );
    out
}

/// `predicates` with the supertraits each implies, transitively, without
/// repeats.
pub fn elaborate(sigs: &Signatures, predicates: &[Predicate]) -> Vec<Predicate> {
    let mut out: Vec<Predicate> = Vec::new();
    let mut stack: Vec<Predicate> = predicates.iter().rev().copied().collect();
    while let Some(p) = stack.pop() {
        if out.contains(&p) {
            continue;
        }
        out.push(p);
        let Some(sig) = sigs.traits.get(p.trait_ref.trait_) else {
            continue;
        };
        let args: Vec<Ty> = std::iter::once(p.self_ty)
            .chain(p.trait_ref.args.iter().copied())
            .collect();
        for sup in sig.supertraits.iter().rev() {
            stack.push(sup.subst(&args).with_self(p.self_ty));
        }
    }
    out
}

/// Binds the parameters in `pattern` (an `impl` block's header, in terms
/// of its parameters) so that it is `ty`: whether it can be. Other
/// parameters in `ty` are rigid types.
pub fn match_ty(pattern: Ty, ty: Ty, binds: &mut [Option<Ty>]) -> bool {
    if pattern == ty && !pattern.has_params() {
        return true;
    }
    match (pattern.kind(), ty.kind()) {
        (TyKind::Param(p), _) => match binds.get(p.index as usize) {
            Some(Some(bound)) => *bound == ty,
            Some(None) => {
                binds[p.index as usize] = Some(ty);
                true
            }
            None => pattern == ty,
        },
        (TyKind::Error, _) | (_, TyKind::Error) => true,
        (TyKind::Adt(a, xs), TyKind::Adt(b, ys)) => {
            a == b && xs.iter().zip(ys).all(|(&x, &y)| match_ty(x, y, binds))
        }
        (TyKind::Tuple(xs), TyKind::Tuple(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys).all(|(&x, &y)| match_ty(x, y, binds))
        }
        (TyKind::Ref(m, x), TyKind::Ref(n, y)) | (TyKind::RawPtr(m, x), TyKind::RawPtr(n, y)) => {
            m == n && match_ty(x, y, binds)
        }
        _ => pattern == ty,
    }
}

/// The generic arguments for which `impl` block `imp`'s header is
/// `goal`, if there are any.
pub fn match_impl(
    items: &ItemTree,
    sigs: &Signatures,
    imp: ImplId,
    goal: Predicate,
) -> Option<Vec<Ty>> {
    let trait_ref = sigs.impl_traits.get(imp)?;
    if trait_ref.trait_ != goal.trait_ref.trait_ {
        return None;
    }
    let self_ty = *sigs.impls.get(imp)?;
    let mut binds = vec![None; items.impls[imp].generics.len()];
    if !match_ty(self_ty, goal.self_ty, &mut binds) {
        return None;
    }
    for (&x, &y) in trait_ref.args.iter().zip(goal.trait_ref.args) {
        if !match_ty(x, y, &mut binds) {
            return None;
        }
    }
    Some(binds.into_iter().map(|b| b.unwrap_or(Ty::ERROR)).collect())
}

/// Whether `goal` holds given `env`, the elaborated bounds in scope, in
/// which generic parameters are rigid: from a bound, or from an `impl`
/// block whose own bounds hold.
pub fn holds(items: &ItemTree, sigs: &Signatures, env: &[Predicate], goal: Predicate) -> bool {
    holds_at(items, sigs, env, goal, 0)
}

fn holds_at(
    items: &ItemTree,
    sigs: &Signatures,
    env: &[Predicate],
    goal: Predicate,
    depth: u32,
) -> bool {
    if goal.self_ty == Ty::ERROR || goal.trait_ref.args.contains(&Ty::ERROR) {
        return true;
    }
    if env.contains(&goal) || depth > 64 {
        return true;
    }
    if items.traits[goal.trait_ref.trait_].lang == Some(flux_intern::sym::sized) {
        return true;
    }
    let impls = items.trait_impls.get(&goal.trait_ref.trait_);
    impls.into_iter().flatten().any(|&imp| {
        let Some(args) = match_impl(items, sigs, imp, goal) else {
            return false;
        };
        let nested = sigs.impl_predicates.get(imp).into_iter().flatten();
        nested
            .copied()
            .all(|p| holds_at(items, sigs, env, p.subst(&args), depth + 1))
    })
}

/// The function a call of `instance` calls: for a trait's method with
/// known types, the method of the `impl` block for them, or the trait's
/// own default; any other function is itself.
pub fn resolve_instance(items: &ItemTree, sigs: &Signatures, instance: Instance) -> Instance {
    let data = &items.functions[instance.function];
    let Some(Owner::Trait(t)) = data.owner else {
        return instance;
    };
    let count = 1 + items.traits[t].generics.len();
    let (header, own) = instance.args.split_at(count.min(instance.args.len()));
    let goal = Predicate {
        self_ty: header[0],
        trait_ref: TraitRef {
            trait_: t,
            name: items.traits[t].name,
            args: Ty::list(&header[1..]),
        },
    };
    let impls = items.trait_impls.get(&t).into_iter().flatten();
    for &imp in impls {
        let Some(mut args) = match_impl(items, sigs, imp, goal) else {
            continue;
        };
        let found = items.impls[imp]
            .functions
            .iter()
            .copied()
            .find(|&f| items.functions[f].name == data.name);
        if let Some(function) = found {
            args.extend_from_slice(own);
            return Instance {
                function,
                args: Ty::list(&args),
            };
        }
        // The trait's default.
        return instance;
    }
    instance
}

/// Checks each trait's `impl` blocks against the trait: that they define
/// its required methods and no others, with its signatures; that they
/// are `unsafe` exactly when it is; that their types implement its
/// supertraits; and that no two of them overlap.
pub fn check_impls(items: &ItemTree, sigs: &Signatures, diagnostics: &mut Vec<Diagnostic>) {
    for (id, imp) in items.impls.iter() {
        let Some(t) = imp.trait_ else {
            continue;
        };
        let mut error = |range: TextRange, message: String| {
            diagnostics.push(Diagnostic {
                file: imp.file,
                range,
                message,
            });
        };
        let data = &items.traits[t];
        match (imp.is_unsafe, data.is_unsafe) {
            (true, false) => error(
                imp.def_range,
                format!("implementing the trait `{}` is not unsafe", data.name),
            ),
            (false, true) => error(
                imp.def_range,
                format!(
                    "the trait `{}` requires an `unsafe impl` declaration",
                    data.name
                ),
            ),
            _ => {}
        }
        for &f in &imp.functions {
            let function = &items.functions[f];
            match data.function(items, function.name) {
                Some(required) => check_method(items, sigs, id, f, required, &mut error),
                None => error(
                    function.item_range,
                    format!(
                        "method `{}` is not a member of trait `{}`",
                        function.name, data.name
                    ),
                ),
            }
        }
        let missing: Vec<String> = data
            .functions
            .iter()
            .map(|&f| &items.functions[f])
            .filter(|f| !f.has_body)
            .filter(|f| {
                !imp.functions
                    .iter()
                    .any(|&g| items.functions[g].name == f.name)
            })
            .map(|f| format!("`{}`", f.name))
            .collect();
        if !missing.is_empty() {
            error(
                imp.def_range,
                format!(
                    "not all trait items implemented, missing: {}",
                    missing.join(", ")
                ),
            );
        }
        // The supertraits must hold for the block's type, given its
        // bounds.
        if let (Some(&trait_ref), Some(&self_ty)) = (sigs.impl_traits.get(id), sigs.impls.get(id)) {
            let env = elaborate(sigs, sigs.impl_predicates.get(id).map_or(&[], |p| p));
            let args: Vec<Ty> = std::iter::once(self_ty)
                .chain(trait_ref.args.iter().copied())
                .collect();
            for sup in sigs.traits.get(t).map_or(&[][..], |s| &s.supertraits) {
                let goal = sup.subst(&args).with_self(self_ty);
                if !holds(items, sigs, &env, goal) {
                    error(
                        imp.self_range,
                        format!("the trait bound `{goal}` is not satisfied"),
                    );
                }
            }
        }
    }
    check_overlap(items, sigs, diagnostics);
}

/// Checks method `f` of trait `impl` block `imp` against the trait's
/// method `required`, of which it is the definition.
fn check_method(
    items: &ItemTree,
    sigs: &Signatures,
    imp: ImplId,
    f: FunctionId,
    required: FunctionId,
    error: &mut impl FnMut(TextRange, String),
) {
    let (function, decl) = (&items.functions[f], &items.functions[required]);
    let name = function.name;
    let Some(t) = items.impls[imp].trait_ else {
        return;
    };
    let trait_name = items.traits[t].name;
    match (function.has_self, decl.has_self) {
        (true, false) => {
            return error(
                function.def_range,
                format!(
                    "method `{name}` has a `{}` declaration in the impl, but not in the trait",
                    self_text(&function.params[0].ty)
                ),
            );
        }
        (false, true) => {
            return error(
                function.def_range,
                format!(
                    "method `{name}` has a `{}` declaration in the trait, but not in the impl",
                    self_text(&decl.params[0].ty)
                ),
            );
        }
        _ => {}
    }
    if function.params.len() != decl.params.len() {
        let range = match (function.params.first(), function.params.last()) {
            (Some(first), Some(last)) => TextRange::new(first.whole.start(), last.whole.end()),
            _ => TextRange::empty(function.param_list.start() + 1),
        };
        let (got, want) = (function.params.len(), decl.params.len());
        return error(
            range,
            format!(
                "method `{name}` has {got} parameter{} but the declaration in trait `{trait_name}::{name}` has {want}",
                if got == 1 { "" } else { "s" }
            ),
        );
    }
    let (got, want) = (function.generics.len(), decl.generics.len());
    if got != want {
        let range = match (
            function.generics.params.first(),
            function.generics.params.last(),
        ) {
            (Some(first), Some(last)) => TextRange::new(first.range.start(), last.range.end()),
            _ => TextRange::empty(function.name_range.end()),
        };
        let params = |n: usize| format!("{n} type parameter{}", if n == 1 { "" } else { "s" });
        return error(
            range,
            format!(
                "method `{name}` has {} but its trait declaration has {}",
                params(got),
                params(want)
            ),
        );
    }
    let (Some(&self_ty), Some(trait_ref)) = (sigs.impls.get(imp), sigs.impl_traits.get(imp)) else {
        return;
    };
    // The trait's signature for this block: its `Self` and parameters
    // replaced by the block's, and the method's own by this method's.
    let first = items.impls[imp].generics.len();
    let mut args = vec![self_ty];
    args.extend_from_slice(trait_ref.args);
    args.extend(crate::params_of(&function.generics, first));
    let (sig, expected) = (&sigs[f], sigs[required].subst(&args));
    let types = sig.params.iter().zip(&expected.params);
    let ranges = function.params.iter().map(|p| p.range);
    let ret_range =
        if function.ret == flux_hir::TypeRef::unit() && function.ret_range == function.name_range {
            TextRange::empty(function.param_list.end())
        } else {
            function.ret_range
        };
    let mismatch = types
        .zip(ranges)
        .chain([((&sig.ret, &expected.ret), ret_range)])
        .find(|((got, want), _)| got != want && **got != Ty::ERROR && **want != Ty::ERROR);
    if let Some((_, range)) = mismatch {
        error(
            range,
            format!("method `{name}` has an incompatible type for trait"),
        );
    }
    // Its bounds may not ask more than the trait's do (E0276).
    let own = sigs.fn_predicates.get(f).map_or(&[][..], |p| p);
    if own.len() != function.generics.predicates.len() {
        return;
    }
    let mut env: Vec<Predicate> = sigs
        .impl_predicates
        .get(imp)
        .map_or(&[][..], |p| p)
        .to_vec();
    let declared = sigs.fn_predicates.get(required).map_or(&[][..], |p| p);
    env.extend(declared.iter().map(|p| p.subst(&args)));
    let env = elaborate(sigs, &env);
    for (written, &p) in function.generics.predicates.iter().zip(own) {
        if !holds(items, sigs, &env, p) {
            error(
                written.bound.range,
                "impl has stricter requirements than trait".to_owned(),
            );
        }
    }
}

/// Whether `general`'s bounds rule out the types of `specific`, as
/// rustc's coherence does for a blanket `impl<T: Copy> Tr for T` and an
/// `impl Tr for S` with an `S` that is not `Copy`: when `general`'s
/// header matches `specific`'s, with `specific`'s parameters as rigid
/// types, and some bound of `general` does not hold for them.
fn excluded_by_bounds(
    items: &ItemTree,
    sigs: &Signatures,
    general: ImplId,
    specific: ImplId,
) -> bool {
    let (Some(&self_ty), Some(&trait_ref)) =
        (sigs.impls.get(specific), sigs.impl_traits.get(specific))
    else {
        return false;
    };
    let goal = trait_ref.with_self(self_ty);
    let Some(args) = match_impl(items, sigs, general, goal) else {
        return false;
    };
    let env = elaborate(sigs, sigs.impl_predicates.get(specific).map_or(&[], |p| p));
    let bounds = sigs.impl_predicates.get(general).map_or(&[][..], |p| p);
    bounds
        .iter()
        .any(|p| !holds(items, sigs, &env, p.subst(&args)))
}

/// `self`, `&self` or `&mut self`, for a `self` parameter of type `ty`.
fn self_text(ty: &flux_hir::TypeRef) -> &'static str {
    match ty {
        flux_hir::TypeRef::Ref { mutable: true, .. } => "&mut self",
        flux_hir::TypeRef::Ref { .. } => "&self",
        _ => "self",
    }
}

/// Reports traits that are their own supertraits, through any others,
/// once per cycle, at the first trait's bound that leads into it.
pub fn check_supertrait_cycles(
    items: &ItemTree,
    sigs: &Signatures,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let mut in_cycle: Vec<TraitId> = Vec::new();
    let mut found_any = false;
    for (start, data) in items.traits.iter() {
        if in_cycle.contains(&start) {
            continue;
        }
        let supers = sigs.traits.get(start).map_or(&[][..], |s| &s.supertraits);
        for (i, sup) in supers.iter().enumerate() {
            // A path from `sup` back to `start`.
            let mut path = vec![sup.trait_];
            let mut seen = vec![sup.trait_];
            let found = reaches(sigs, sup.trait_, start, &mut seen, &mut path);
            if !found {
                continue;
            }
            in_cycle.extend(path);
            in_cycle.push(start);
            // `Sup` in `trait Tr: Sup`, or in its `where Self: Sup`.
            let in_where = data.generics.predicates.iter().filter(|p| {
                matches!(&p.ty, flux_hir::TypeRef::Path(path) if path.as_name() == Some(kw::SelfUpper))
            });
            let range = data
                .supertraits
                .iter()
                .map(|b| b.range)
                .chain(in_where.map(|p| p.bound.range))
                .nth(i)
                .unwrap_or(data.name_range);
            found_any = true;
            diagnostics.push(Diagnostic {
                file: data.loc.file,
                range,
                message: format!(
                    "cycle detected when computing the super clauses of `{}`",
                    data.name
                ),
            });
            break;
        }
    }
    found_any
}

fn reaches(
    sigs: &Signatures,
    from: TraitId,
    to: TraitId,
    seen: &mut Vec<TraitId>,
    path: &mut Vec<TraitId>,
) -> bool {
    if from == to {
        return true;
    }
    let supers = sigs.traits.get(from).map_or(&[][..], |s| &s.supertraits);
    for sup in supers.iter() {
        if seen.contains(&sup.trait_) && sup.trait_ != to {
            continue;
        }
        seen.push(sup.trait_);
        path.push(sup.trait_);
        if reaches(sigs, sup.trait_, to, seen, path) {
            return true;
        }
        path.pop();
    }
    false
}

/// Reports bounds on types without generic parameters that do not hold,
/// such as `where S: Tr` for an `S` that does not implement `Tr`.
pub fn check_trivial_bounds(
    items: &ItemTree,
    sigs: &Signatures,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut check = |generics: &Generics, predicates: &[Predicate], file| {
        // Predicates follow the written bounds, less those that did not
        // resolve; match them by their written types' text.
        for (written, &p) in generics.predicates.iter().zip(predicates) {
            if p.self_ty.has_params() || p.trait_ref.args.iter().any(|a| a.has_params()) {
                continue;
            }
            if !holds(items, sigs, &[], p) {
                diagnostics.push(Diagnostic {
                    file,
                    range: TextRange::new(written.ty_range.start(), written.bound.range.end()),
                    message: format!("the trait bound `{p}` is not satisfied"),
                });
            }
        }
    };
    for (id, f) in items.functions.iter() {
        let predicates = sigs.fn_predicates.get(id).map_or(&[][..], |p| p);
        if predicates.len() == f.generics.predicates.len() {
            check(&f.generics, predicates, f.loc.file);
        }
    }
    for (id, imp) in items.impls.iter() {
        let predicates = sigs.impl_predicates.get(id).map_or(&[][..], |p| p);
        if predicates.len() == imp.generics.predicates.len() {
            check(&imp.generics, predicates, imp.file);
        }
    }
}

/// Reports `impl` blocks of one trait for types that may be the same.
fn check_overlap(items: &ItemTree, sigs: &Signatures, diagnostics: &mut Vec<Diagnostic>) {
    let mut traits: Vec<(&TraitId, &Vec<ImplId>)> = items.trait_impls.iter().collect();
    traits.sort_by_key(|(t, _)| t.index());
    for (_, impls) in traits {
        for (i, &later) in impls.iter().enumerate() {
            let (Some(&b), Some(b_ref)) = (sigs.impls.get(later), sigs.impl_traits.get(later))
            else {
                continue;
            };
            let earlier = impls[..i].iter().find(|&&a| {
                let (Some(&a_ty), Some(a_ref)) = (sigs.impls.get(a), sigs.impl_traits.get(a))
                else {
                    return false;
                };
                crate::overlap(a_ty, b)
                    && a_ref
                        .args
                        .iter()
                        .zip(b_ref.args)
                        .all(|(&x, &y)| crate::overlap(x, y))
                    && !excluded_by_bounds(items, sigs, a, later)
                    && !excluded_by_bounds(items, sigs, later, a)
            });
            let Some(&earlier) = earlier else {
                continue;
            };
            // The type both are for: the more specific one's.
            let ty = if b.has_params() {
                sigs.impls[earlier]
            } else {
                b
            };
            let imp = &items.impls[later];
            diagnostics.push(Diagnostic {
                file: imp.file,
                range: imp.def_range,
                message: format!(
                    "conflicting implementations of trait `{}` for type `{ty}`",
                    b_ref.name
                ),
            });
        }
    }
}
