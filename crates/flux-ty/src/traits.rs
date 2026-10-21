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

use crate::ty::{AliasTy, GenericArgs, Mutability, Ty, TyKind};
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

/// How rustc reports that `p` does not hold: by its trait's
/// `#[diagnostic::on_unimplemented]` message if it has one ("cannot add
/// `i32` to `u8`"), else "the trait bound `S: Tr` is not satisfied".
/// `describe` shows types; `path` is the trait as messages name it.
pub fn unmet_message(
    items: &ItemTree,
    p: Predicate,
    describe: &mut dyn FnMut(Ty) -> String,
    path: &str,
) -> String {
    let data = &items.traits[p.trait_ref.trait_];
    let Some(template) = &data.on_unimplemented else {
        let args: Vec<String> = p.trait_ref.args.iter().map(|&a| describe(a)).collect();
        let args = if args.is_empty() {
            String::new()
        } else {
            format!("<{}>", args.join(", "))
        };
        return format!(
            "the trait bound `{}: {path}{args}` is not satisfied",
            describe(p.self_ty)
        );
    };
    let mut out = String::new();
    let mut rest = &template[..];
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        rest = &rest[open + 1..];
        let Some(close) = rest.find('}') else {
            out.push('{');
            break;
        };
        let name = &rest[..close];
        rest = &rest[close + 1..];
        let param = data
            .generics
            .params
            .iter()
            .position(|g| g.name.as_str() == name);
        match (name, param) {
            ("Self", _) => out.push_str(&describe(p.self_ty)),
            ("This", _) => out.push_str(path),
            (_, Some(i)) if i < p.trait_ref.args.len() => {
                out.push_str(&describe(p.trait_ref.args[i]));
            }
            _ => {
                out.push('{');
                out.push_str(name);
                out.push('}');
            }
        }
    }
    out.push_str(rest);
    out
}

/// [`unmet_message`] for a check outside a body, which names types
/// and traits as they print.
fn unmet(items: &ItemTree, p: Predicate) -> String {
    unmet_message(
        items,
        p,
        &mut |t| t.to_string(),
        items.traits[p.trait_ref.trait_].name.as_str(),
    )
}

/// `<T as Tr>::Out == ty`: what an associated type is, from a bound's
/// `Tr<Out = ty>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Projection {
    pub alias: AliasTy,
    pub ty: Ty,
}

impl Projection {
    pub fn subst(self, args: &[Ty]) -> Projection {
        let alias = match Ty::alias(self.alias).subst(args).kind() {
            TyKind::Alias(alias) => alias,
            _ => unreachable!("substituting keeps an alias one"),
        };
        Projection {
            alias,
            ty: self.ty.subst(args),
        }
    }
}

impl fmt::Display for Projection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} == {}", Ty::alias(self.alias), self.ty)
    }
}

/// A trait's resolved header.
#[derive(Debug, Clone, Default)]
pub struct TraitSig {
    /// `Sup` in `trait Tr: Sup`, in terms of the trait's parameters,
    /// `Self` first.
    pub supertraits: Box<[TraitRef]>,
    /// The default of each of its own parameters, if it has one, in terms
    /// of the parameters before it: `Self` in `trait Add<Rhs = Self>`.
    pub defaults: Box<[Option<Ty>]>,
    /// The bounds on its associated types: `Bound` in `type Out: Bound;`,
    /// by name.
    pub type_bounds: Box<[(Symbol, TraitRef)]>,
    /// What `Self: Tr` says its associated types and its supertraits' are:
    /// `Out = i32` in `trait Tr: Sup<Out = i32>` or `type N: Add<Output = i32>;`.
    pub implied: Box<[Projection]>,
}

/// The default of each trait's parameters, for bounds that leave them out.
pub type TraitDefaults = flux_arena::ArenaMap<flux_hir::Trait, Box<[Option<Ty>]>>;

/// Resolved bounds: what types implement, and what associated types are.
#[derive(Debug, Clone, Default)]
pub struct Bounds {
    pub predicates: Vec<Predicate>,
    pub projections: Vec<Projection>,
}

/// The trait bound `bound` on `self_ty`, written in `module`: its trait,
/// with the generic arguments written after its last segment, one for
/// each of the trait's parameters, and the associated types it binds.
/// `None` if it did not resolve (reported in `errors`).
pub fn resolve_bound(
    items: &ItemTree,
    module: ModuleId,
    bound: &Bound,
    self_ty: Ty,
    scope: TypeScope<'_>,
    errors: &mut Vec<(TextRange, String)>,
) -> Option<(TraitRef, Vec<Projection>)> {
    let (t, error) = items.resolve_trait(module, bound);
    if let Some(error) = error {
        errors.push(error);
    }
    trait_ref(items, module, t?, bound, self_ty, scope, errors)
}

/// Trait `t`, named by `bound` for `self_ty`, with the generic arguments
/// written after its last segment, the defaults of those left out, and
/// the associated types it binds.
pub fn trait_ref(
    items: &ItemTree,
    module: ModuleId,
    t: TraitId,
    bound: &Bound,
    self_ty: Ty,
    scope: TypeScope<'_>,
    errors: &mut Vec<(TextRange, String)>,
) -> Option<(TraitRef, Vec<Projection>)> {
    let data = &items.traits[t];
    let path = &bound.path;
    let last = path.segments.len().saturating_sub(1);
    let at = path.range(last, bound.range);
    let want = data.generics.len();
    let written = path.args(last);
    let mut args: Vec<Ty> = written.map_or(Vec::new(), |written| {
        written
            .types
            .iter()
            .zip(&written.ranges)
            .map(|(ty, &range)| resolve_type(items, module, ty, scope, range, errors))
            .collect()
    });
    let defaults = scope.defaults.and_then(|d| d.get(t)).map_or(&[][..], |d| d);
    let required = data
        .generics
        .params
        .iter()
        .take_while(|p| p.default.is_none())
        .count();
    if args.len() > want {
        let message = if required < want {
            let n = want;
            format!(
                "trait takes at most {n} generic argument{} but {} generic argument{} {} supplied",
                if n == 1 { "" } else { "s" },
                args.len(),
                if args.len() == 1 { "" } else { "s" },
                if args.len() == 1 { "was" } else { "were" }
            )
        } else {
            wrong_arg_count("trait", want, args.len())
        };
        errors.push((at, message));
        return None;
    }
    if args.len() < required {
        if args.is_empty() {
            errors.push((at, format!("missing generics for trait `{}`", data.name)));
        } else {
            errors.push((at, wrong_arg_count("trait", want, args.len())));
        }
        return None;
    }
    // The defaults of those left out, in terms of `Self` and the ones
    // before.
    while args.len() < want {
        let i = args.len();
        let default = defaults.get(i).copied().flatten().unwrap_or(Ty::ERROR);
        let before: Vec<Ty> = std::iter::once(self_ty)
            .chain(args.iter().copied())
            .collect();
        args.push(default.subst(&before));
    }
    if args.contains(&Ty::ERROR) {
        return None;
    }
    let trait_ref = TraitRef {
        trait_: t,
        name: data.name,
        args: Ty::list(&args),
    };
    let mut projections = Vec::new();
    for binding in written.map_or(&[][..], |w| &w.bindings) {
        if !data.types.iter().any(|ty| ty.name == binding.name) {
            errors.push((
                binding.name_range,
                format!(
                    "associated type `{}` not found for `{}`",
                    binding.name, data.name
                ),
            ));
            continue;
        }
        let ty = resolve_type(items, module, &binding.ty, scope, binding.ty_range, errors);
        projections.push(Projection {
            alias: alias_of(trait_ref, self_ty, binding.name),
            ty,
        });
    }
    Some((trait_ref, projections))
}

/// `<self_ty as trait_ref>::name`.
pub fn alias_of(trait_ref: TraitRef, self_ty: Ty, name: Symbol) -> AliasTy {
    let args: Vec<Ty> = std::iter::once(self_ty)
        .chain(trait_ref.args.iter().copied())
        .collect();
    AliasTy {
        trait_: trait_ref.trait_,
        trait_name: trait_ref.name,
        name,
        args: Ty::list(&args),
    }
}

/// The bounds of `generics`, in its list and `where` clause, written in
/// `module`; those that did not resolve are left out (reported in
/// `errors`).
pub fn resolve_predicates(
    items: &ItemTree,
    module: ModuleId,
    generics: &Generics,
    scope: TypeScope<'_>,
    errors: &mut Vec<(TextRange, String)>,
) -> Bounds {
    let mut out = Bounds::default();
    for p in generics.predicates.iter() {
        let self_ty = resolve_type(items, module, &p.ty, scope, p.ty_range, errors);
        let Some((trait_ref, projections)) =
            resolve_bound(items, module, &p.bound, self_ty, scope, errors)
        else {
            continue;
        };
        if self_ty != Ty::ERROR {
            out.predicates.push(trait_ref.with_self(self_ty));
            out.projections.extend(projections);
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

/// The bounds of an associated item's owner: an `impl` block's, or a
/// trait's (`Self: Tr` first).
pub fn owner_predicates(
    items: &ItemTree,
    sigs: &Signatures,
    owner: Option<Owner>,
) -> Vec<Predicate> {
    let mut out = Vec::new();
    match owner {
        Some(Owner::Impl(imp)) => {
            out.extend(sigs.impl_predicates.get(imp).into_iter().flatten().copied());
        }
        Some(Owner::Trait(t)) => {
            out.push(trait_self_predicate(items, t));
            out.extend(sigs.trait_predicates.get(t).into_iter().flatten().copied());
        }
        None => {}
    }
    out
}

/// The bounds a function's body may rely on: its owner's (`Self: Tr` in
/// a trait), then its own, in terms of all its parameters, not yet
/// elaborated with supertraits.
pub fn predicates_of(items: &ItemTree, sigs: &Signatures, function: FunctionId) -> Vec<Predicate> {
    let mut out = owner_predicates(items, sigs, items.functions[function].owner);
    out.extend(
        sigs.fn_predicates
            .get(function)
            .into_iter()
            .flatten()
            .copied(),
    );
    out
}

/// What the bounds in scope in `function` say associated types are: its
/// owner's, then its own, then what their traits imply.
pub fn projections_of(
    items: &ItemTree,
    sigs: &Signatures,
    function: FunctionId,
) -> Vec<Projection> {
    let mut out = declared_projections(items, sigs, function);
    out.extend(implied_projections(
        sigs,
        &elaborate(sigs, &predicates_of(items, sigs, function)),
    ));
    out
}

/// The associated types the bounds of `function` and its block bind,
/// as written: what a call must show.
pub fn declared_projections(
    items: &ItemTree,
    sigs: &Signatures,
    function: FunctionId,
) -> Vec<Projection> {
    let mut out = Vec::new();
    match items.functions[function].owner {
        Some(Owner::Impl(imp)) => {
            out.extend(
                sigs.impl_projections
                    .get(imp)
                    .into_iter()
                    .flatten()
                    .copied(),
            );
        }
        Some(Owner::Trait(t)) => {
            out.extend(sigs.trait_projections.get(t).into_iter().flatten().copied());
        }
        None => {}
    }
    out.extend(
        sigs.fn_projections
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
        // `T: Tr` with `type Out: Bound` in `Tr` gives `T::Out: Bound`,
        // to a depth: `type Inner: Tr;` would go on forever.
        if alias_depth(p.self_ty) >= 4 {
            continue;
        }
        for &(name, bound) in sig.type_bounds.iter().rev() {
            let alias = alias_of(p.trait_ref, p.self_ty, name);
            stack.push(bound.subst(&args).with_self(Ty::alias(alias)));
        }
    }
    out
}

/// What the associated types in `env`, elaborated bounds, are by what
/// their traits say: `T::N as Add>::Output == i32` from `T: Num` with
/// `type N: Add<Output = i32>;` in `Num`.
pub fn implied_projections(sigs: &Signatures, env: &[Predicate]) -> Vec<Projection> {
    let mut out = Vec::new();
    for p in env {
        let Some(sig) = sigs.traits.get(p.trait_ref.trait_) else {
            continue;
        };
        if sig.implied.is_empty() {
            continue;
        }
        let args: Vec<Ty> = std::iter::once(p.self_ty)
            .chain(p.trait_ref.args.iter().copied())
            .collect();
        out.extend(sig.implied.iter().map(|q| q.subst(&args)));
    }
    out
}

/// How deeply associated types nest in `ty`'s outermost one:
/// 2 for `<T::A as Tr>::B`.
fn alias_depth(ty: Ty) -> u32 {
    match ty.kind() {
        TyKind::Alias(alias) => 1 + alias_depth(alias.args[0]),
        _ => 0,
    }
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
        (TyKind::Alias(a), TyKind::Alias(b)) => {
            a.trait_ == b.trait_
                && a.name == b.name
                && a.args
                    .iter()
                    .zip(b.args)
                    .all(|(&x, &y)| match_ty(x, y, binds))
        }
        _ => pattern == ty,
    }
}

/// What `impl` block `imp` says its associated type `name` is, in terms
/// of its parameters.
pub fn impl_type(sigs: &Signatures, imp: ImplId, name: Symbol) -> Option<Ty> {
    let types = sigs.impl_types.get(imp)?;
    types.iter().find(|(n, _)| *n == name).map(|&(_, ty)| ty)
}

/// `ty` with each associated type replaced by what it is, where that is
/// known: from `env`, the bounds in scope, or from the `impl` block for
/// its types. The others stay, as types of their own.
pub fn normalize(items: &ItemTree, sigs: &Signatures, env: &[Projection], ty: Ty) -> Ty {
    normalize_at(items, sigs, env, ty, 0)
}

fn normalize_at(items: &ItemTree, sigs: &Signatures, env: &[Projection], ty: Ty, depth: u32) -> Ty {
    if !ty.has_aliases() || depth > 64 {
        return ty;
    }
    let go = |t: Ty| normalize_at(items, sigs, env, t, depth + 1);
    match ty.kind() {
        TyKind::Alias(alias) => {
            let args: Vec<Ty> = alias.args.iter().map(|&a| go(a)).collect();
            let alias = AliasTy {
                args: Ty::list(&args),
                ..alias
            };
            if let Some(p) = env.iter().find(|p| p.alias == alias) {
                return go(p.ty);
            }
            let goal = Predicate {
                self_ty: args[0],
                trait_ref: TraitRef {
                    trait_: alias.trait_,
                    name: alias.trait_name,
                    args: Ty::list(&args[1..]),
                },
            };
            let impls = items.trait_impls.get(&alias.trait_).into_iter().flatten();
            for &imp in impls {
                if let Some(impl_args) = match_impl(items, sigs, imp, goal)
                    && let Some(t) = impl_type(sigs, imp, alias.name)
                {
                    return go(t.subst(&impl_args));
                }
            }
            Ty::alias(alias)
        }
        TyKind::Tuple(list) => Ty::tuple(&list.iter().map(|&t| go(t)).collect::<Vec<_>>()),
        TyKind::Adt(adt, list) => {
            let list: Vec<Ty> = list.iter().map(|&t| go(t)).collect();
            Ty::adt(adt.strukt, adt.name, &list)
        }
        TyKind::Ref(m, t) => Ty::new(TyKind::Ref(m, go(t))),
        TyKind::RawPtr(m, t) => Ty::raw_ptr(m, go(t)),
        _ => ty,
    }
}

/// The signature of `function` with generic arguments `args`, its
/// associated types normalized: an instance's, for code generation.
pub fn instance_sig(
    items: &ItemTree,
    sigs: &Signatures,
    function: FunctionId,
    args: &[Ty],
) -> crate::FnSig {
    let sig = &sigs[function];
    crate::FnSig {
        params: sig
            .params
            .iter()
            .map(|&p| instance_ty(items, sigs, p, args))
            .collect(),
        ret: instance_ty(items, sigs, sig.ret, args),
    }
}

/// `ty` with generic arguments `args` substituted and its associated
/// types normalized: a type of an instance, for code generation.
pub fn instance_ty(items: &ItemTree, sigs: &Signatures, ty: Ty, args: &[Ty]) -> Ty {
    normalize(items, sigs, &[], ty.subst(args))
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
    holds_at(items, sigs, env, goal, &mut Vec::new())
}

/// [`holds`], within the goals `stack` is proving. As in rustc, a goal
/// that needs itself does not hold, nor does one that needs ever larger
/// types.
fn holds_at(
    items: &ItemTree,
    sigs: &Signatures,
    env: &[Predicate],
    goal: Predicate,
    stack: &mut Vec<Predicate>,
) -> bool {
    if goal.self_ty == Ty::ERROR || goal.trait_ref.args.contains(&Ty::ERROR) {
        return true;
    }
    if env.contains(&goal) {
        return true;
    }
    if stack.len() > 256 || stack.contains(&goal) {
        return false;
    }
    if items.traits[goal.trait_ref.trait_].lang == Some(flux_intern::sym::sized)
        || is_builtin_copy(items, goal.trait_ref.trait_, goal.self_ty)
    {
        return true;
    }
    let impls = items.trait_impls.get(&goal.trait_ref.trait_);
    stack.push(goal);
    let holds = impls.into_iter().flatten().any(|&imp| {
        let Some(args) = match_impl(items, sigs, imp, goal) else {
            return false;
        };
        let nested = sigs.impl_predicates.get(imp).into_iter().flatten();
        nested
            .copied()
            .all(|p| holds_at(items, sigs, env, p.subst(&args), stack))
    });
    stack.pop();
    holds
}

/// Whether `ty: t` holds by the compiler's say: shared references are
/// `Copy` and `Clone`, which `core` cannot write until functions may
/// return references.
pub fn is_builtin_copy(items: &ItemTree, t: TraitId, ty: Ty) -> bool {
    matches!(ty.kind(), TyKind::Ref(Mutability::Not, _))
        && matches!(items.traits[t].lang, Some(l) if l.as_str() == "copy" || l.as_str() == "clone")
}

/// Whether values of `ty` are copied rather than moved, given `env`, the
/// elaborated bounds in scope: those of types that implement `Copy`.
pub fn is_copy(items: &ItemTree, sigs: &Signatures, env: &[Predicate], ty: Ty) -> bool {
    match ty.kind() {
        TyKind::Tuple(elements) => elements.iter().all(|&t| is_copy(items, sigs, env, t)),
        TyKind::Ref(Mutability::Mut, _) => false,
        TyKind::Adt(..) | TyKind::Param(_) | TyKind::Alias(_) => {
            let Some(&copy) = items.lang_traits.get(&Symbol::intern("copy")) else {
                return false;
            };
            let goal = Predicate {
                self_ty: ty,
                trait_ref: TraitRef {
                    trait_: copy,
                    name: items.traits[copy].name,
                    args: Ty::list(&[]),
                },
            };
            holds(items, sigs, env, goal)
        }
        _ => true,
    }
}

/// The function a call of `instance` calls: for a trait's method with
/// known types, the method of the `impl` block for them, or the trait's
/// own default; any other function is itself.
pub fn resolve_instance(items: &ItemTree, sigs: &Signatures, instance: Instance) -> Instance {
    let instance = Instance {
        args: Ty::list(
            &instance
                .args
                .iter()
                .map(|&a| normalize(items, sigs, &[], a))
                .collect::<Vec<_>>(),
        ),
        ..instance
    };
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
pub fn check_impls(
    items: &ItemTree,
    sigs: &Signatures,
    diagnostics: &mut Vec<Diagnostic>,
    check_core: bool,
) {
    // As in rustc, a trait with conflicting impls, or `Copy` with one
    // for a type that cannot be, has its impls checked no further.
    let mut skipped = check_overlap(items, sigs, diagnostics, check_core);
    if check_copy_impls(items, sigs, diagnostics, check_core)
        && let Some(&copy) = items.lang_traits.get(&Symbol::intern("copy"))
    {
        skipped.push(copy);
    }
    for (id, imp) in items.impls.iter() {
        let Some(t) = imp.trait_ else {
            continue;
        };
        // `core`'s are checked by its tests, not on every build.
        if !check_core && !items.in_main_crate(imp.module) || skipped.contains(&t) {
            continue;
        }
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
        let has_type = |name| data.types.iter().any(|t| t.name == name);
        for &f in &imp.functions {
            let function = &items.functions[f];
            match data.function(items, function.name) {
                Some(required) => check_method(items, sigs, id, f, required, &mut error),
                None if has_type(function.name) => error(
                    function.item_range,
                    format!(
                        "item `{}` is an associated method, which doesn't match its trait `{}`",
                        function.name, data.name
                    ),
                ),
                None => error(
                    function.item_range,
                    format!(
                        "method `{}` is not a member of trait `{}`",
                        function.name, data.name
                    ),
                ),
            }
        }
        for ty in &imp.types {
            if has_type(ty.name) {
                check_type_bounds(items, sigs, id, ty, &mut error);
            } else if data.function(items, ty.name).is_some() {
                error(
                    ty.item_range,
                    format!(
                        "item `{}` is an associated type, which doesn't match its trait `{}`",
                        ty.name, data.name
                    ),
                );
            } else {
                error(
                    ty.item_range,
                    format!(
                        "type `{}` is not a member of trait `{}`",
                        ty.name, data.name
                    ),
                );
            }
        }
        // Required functions and types not defined, in the trait's order.
        let mut missing: Vec<(u32, Symbol)> = data
            .functions
            .iter()
            .map(|&f| &items.functions[f])
            .filter(|f| !f.has_body)
            .filter(|f| {
                !imp.functions
                    .iter()
                    .any(|&g| items.functions[g].name == f.name)
            })
            .map(|f| (f.def_range.start(), f.name))
            .collect();
        missing.extend(
            data.types
                .iter()
                .filter(|t| !imp.types.iter().any(|u| u.name == t.name))
                .map(|t| (t.item_range.start(), t.name)),
        );
        missing.sort_by_key(|&(at, _)| at);
        let missing: Vec<String> = missing.iter().map(|(_, n)| format!("`{n}`")).collect();
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
                    error(imp.self_range, unmet(items, goal));
                }
            }
        }
    }
}

/// Checks that associated type `ty` of trait `impl` block `imp` meets the
/// bounds its trait puts on it.
fn check_type_bounds(
    items: &ItemTree,
    sigs: &Signatures,
    imp: ImplId,
    ty: &flux_hir::ImplType,
    error: &mut impl FnMut(TextRange, String),
) {
    let (Some(&trait_ref), Some(&self_ty), Some(t)) = (
        sigs.impl_traits.get(imp),
        sigs.impls.get(imp),
        impl_type(sigs, imp, ty.name),
    ) else {
        return;
    };
    let Some(sig) = sigs.traits.get(trait_ref.trait_) else {
        return;
    };
    let args: Vec<Ty> = std::iter::once(self_ty)
        .chain(trait_ref.args.iter().copied())
        .collect();
    let env = elaborate(sigs, sigs.impl_predicates.get(imp).map_or(&[], |p| p));
    let projections = sigs.impl_projections.get(imp).map_or(&[][..], |p| p);
    let t = normalize(items, sigs, projections, t);
    for &(name, bound) in sig.type_bounds.iter() {
        if name != ty.name {
            continue;
        }
        let goal = bound.subst(&args).with_self(t);
        let goal = Predicate {
            self_ty: goal.self_ty,
            trait_ref: TraitRef {
                args: Ty::list(
                    &goal
                        .trait_ref
                        .args
                        .iter()
                        .map(|&a| normalize(items, sigs, projections, a))
                        .collect::<Vec<_>>(),
                ),
                ..goal.trait_ref
            },
        };
        if !holds(items, sigs, &env, goal) {
            error(ty.ty_range, unmet(items, goal));
            return;
        }
    }
    // `type Out: Add<Output = i32>;`: the type's `Output` must be `i32`.
    let self_param = Ty::param(0, flux_intern::kw::SelfUpper);
    for q in sig.implied.iter() {
        let TyKind::Alias(of) = q.alias.args[0].kind() else {
            continue;
        };
        if of.trait_ != trait_ref.trait_ || of.name != ty.name || of.args[0] != self_param {
            continue;
        }
        let q = q.subst(&args);
        let alias = AliasTy {
            args: Ty::list(
                &q.alias
                    .args
                    .iter()
                    .map(|&a| normalize(items, sigs, projections, a))
                    .collect::<Vec<_>>(),
            ),
            ..q.alias
        };
        let found = normalize(items, sigs, projections, Ty::alias(alias));
        let expected = normalize(items, sigs, projections, q.ty);
        if found != expected && found != Ty::ERROR && expected != Ty::ERROR {
            error(
                ty.ty_range,
                format!(
                    "type mismatch resolving `{} == {expected}`",
                    Ty::alias(alias)
                ),
            );
        }
    }
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
    // Without all of the trait's types (reported), its signatures cannot
    // be compared.
    let defined = |name| items.impls[imp].types.iter().any(|ty| ty.name == name);
    if !items.traits[t].types.iter().all(|ty| defined(ty.name)) {
        return;
    }
    // The trait's signature for this block: its `Self` and parameters
    // replaced by the block's, and the method's own by this method's.
    let first = items.impls[imp].generics.len();
    let mut args = vec![self_ty];
    args.extend_from_slice(trait_ref.args);
    args.extend(crate::params_of(&function.generics, first));
    let projections = sigs.impl_projections.get(imp).map_or(&[][..], |p| p);
    let norm = |sig: &crate::FnSig| crate::FnSig {
        params: sig
            .params
            .iter()
            .map(|&p| normalize(items, sigs, projections, p))
            .collect(),
        ret: normalize(items, sigs, projections, sig.ret),
    };
    let (sig, expected) = (&norm(&sigs[f]), norm(&sigs[required].subst(&args)));
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
                    message: unmet(items, p),
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
/// Reports `impl` blocks of a trait that overlap an earlier one (E0119);
/// gives the traits that have any.
fn check_overlap(
    items: &ItemTree,
    sigs: &Signatures,
    diagnostics: &mut Vec<Diagnostic>,
    check_core: bool,
) -> Vec<TraitId> {
    let mut conflicted = Vec::new();
    let mut traits: Vec<(&TraitId, &Vec<ImplId>)> = items.trait_impls.iter().collect();
    traits.sort_by_key(|(t, _)| t.index());
    for (_, impls) in traits {
        for (i, &later) in impls.iter().enumerate() {
            if !check_core && !items.in_main_crate(items.impls[later].module) {
                continue;
            }
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
            if !conflicted.contains(&b_ref.trait_) {
                conflicted.push(b_ref.trait_);
            }
        }
    }
    conflicted
}

/// Reports `Copy` impls for structs with a field that is not `Copy`
/// (E0204); whether there are any.
fn check_copy_impls(
    items: &ItemTree,
    sigs: &Signatures,
    diagnostics: &mut Vec<Diagnostic>,
    check_core: bool,
) -> bool {
    let Some(&copy) = items.lang_traits.get(&Symbol::intern("copy")) else {
        return false;
    };
    let mut any = false;
    for &id in items.trait_impls.get(&copy).into_iter().flatten() {
        let imp = &items.impls[id];
        if !check_core && !items.in_main_crate(imp.module) {
            continue;
        }
        let Some(&self_ty) = sigs.impls.get(id) else {
            continue;
        };
        let env = elaborate(sigs, sigs.impl_predicates.get(id).map_or(&[], |p| p));
        if let TyKind::Adt(adt, args) = self_ty.kind()
            && sigs.structs[adt.strukt]
                .iter()
                .any(|f| !is_copy(items, sigs, &env, f.subst(args)))
        {
            diagnostics.push(Diagnostic {
                file: imp.file,
                range: imp.self_range,
                message: "the trait `Copy` cannot be implemented for this type".into(),
            });
            any = true;
        }
    }
    any
}
