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
    Bound, Diagnostic, FileId, FunctionId, Generics, ImplId, ItemCounts, ItemTree, ModuleId, Owner,
    StructId, TraitId, TypeRef,
};
use flux_intern::{Symbol, kw};
use flux_syntax::TextRange;

use crate::ty::{AliasTy, GenericArgs, Mutability, Ty, TyKind};
use crate::{ConstInstance, Instance, Signatures, TypeScope, resolve_type, wrong_arg_count};

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

/// What the bounds of an associated item's owner say associated types
/// are: as written, then what their traits imply.
pub fn owner_projections(
    items: &ItemTree,
    sigs: &Signatures,
    owner: Option<Owner>,
) -> Vec<Projection> {
    let mut out: Vec<Projection> = match owner {
        Some(Owner::Impl(imp)) => sigs
            .impl_projections
            .get(imp)
            .into_iter()
            .flatten()
            .copied()
            .collect(),
        Some(Owner::Trait(t)) => sigs
            .trait_projections
            .get(t)
            .into_iter()
            .flatten()
            .copied()
            .collect(),
        None => Vec::new(),
    };
    out.extend(implied_projections(
        sigs,
        &elaborate(sigs, &owner_predicates(items, sigs, owner)),
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
/// repeats. The bounds of their associated types (`type Out: Bound;`) are
/// left to [`alias_bounds`], as rustc leaves them: `type Inner: Tr;`
/// would go on forever, and nested ones multiply.
pub fn elaborate(sigs: &Signatures, predicates: &[Predicate]) -> Vec<Predicate> {
    let mut out: Vec<Predicate> = Vec::new();
    let mut seen = rustc_hash::FxHashSet::default();
    let mut stack: Vec<Predicate> = predicates.iter().rev().copied().collect();
    while let Some(p) = stack.pop() {
        if !seen.insert(p) {
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

/// The bounds associated type `ty` has by its trait, elaborated, where
/// `env` says its `Self` implements the trait: `<T as Tr>::Out: Bound`
/// from `type Out: Bound;` in `Tr`, given `T: Tr`. None for other types.
pub fn alias_bounds(sigs: &Signatures, env: &[Predicate], ty: Ty) -> Vec<Predicate> {
    let TyKind::Alias(alias) = ty.kind() else {
        return Vec::new();
    };
    let Some(sig) = sigs.traits.get(alias.trait_) else {
        return Vec::new();
    };
    if !sig.type_bounds.iter().any(|&(name, _)| name == alias.name)
        || !in_env(sigs, env, alias_predicate(alias))
    {
        return Vec::new();
    }
    let bounds: Vec<Predicate> = sig
        .type_bounds
        .iter()
        .filter(|&&(name, _)| name == alias.name)
        .map(|&(_, bound)| bound.subst(alias.args).with_self(ty))
        .collect();
    elaborate(sigs, &bounds)
}

/// Whether `env`, elaborated bounds, has `goal`, or the bounds of its
/// `Self`'s trait do if that is an associated type.
pub fn in_env(sigs: &Signatures, env: &[Predicate], goal: Predicate) -> bool {
    env.contains(&goal)
        || (matches!(goal.self_ty.kind(), TyKind::Alias(_))
            && alias_bounds(sigs, env, goal.self_ty).contains(&goal))
}

/// `Self: Tr` for `<Self as Tr>::Out`.
fn alias_predicate(alias: AliasTy) -> Predicate {
    Predicate {
        self_ty: alias.args[0],
        trait_ref: TraitRef {
            trait_: alias.trait_,
            name: alias.trait_name,
            args: Ty::list(&alias.args[1..]),
        },
    }
}

/// What associated types of associated type `alias` are by its trait's
/// declaration: `<T::N as Add>::Output == i32` for `T::N` from
/// `type N: Add<Output = i32>;`, and what its bounds' traits say.
pub fn alias_projections(sigs: &Signatures, alias: AliasTy) -> Vec<Projection> {
    let Some(sig) = sigs.traits.get(alias.trait_) else {
        return Vec::new();
    };
    let ty = Ty::alias(alias);
    let mut out: Vec<Projection> = sig
        .implied
        .iter()
        .map(|q| q.subst(alias.args))
        .filter(|q| q.alias.args[0] == ty)
        .collect();
    let bounds: Vec<Predicate> = sig
        .type_bounds
        .iter()
        .filter(|&&(name, _)| name == alias.name)
        .map(|&(_, bound)| bound.subst(alias.args).with_self(ty))
        .collect();
    out.extend(implied_projections(sigs, &elaborate(sigs, &bounds)));
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
    if !ty.has_aliases() {
        return ty;
    }
    let mut normalizer = Normalizer {
        items,
        sigs,
        env,
        done: rustc_hash::FxHashMap::default(),
        active: Vec::new(),
    };
    normalizer.go(ty, 0)
}

/// One [`normalize`]: what each type it met normalizes to, as aliases
/// recur in what they normalize to (`<T as Tr>::A` in an `impl` block's
/// `type B`), which would otherwise be normalized again each time.
struct Normalizer<'a> {
    items: &'a ItemTree,
    sigs: &'a Signatures,
    env: &'a [Projection],
    done: rustc_hash::FxHashMap<Ty, Ty>,
    /// The aliases being normalized: one met again within its own
    /// normalization is a cycle, and stays as it is.
    active: Vec<Ty>,
}

impl Normalizer<'_> {
    fn go(&mut self, ty: Ty, depth: u32) -> Ty {
        if !ty.has_aliases() || depth > 64 {
            return ty;
        }
        if let Some(&done) = self.done.get(&ty) {
            return done;
        }
        if self.active.contains(&ty) {
            return ty;
        }
        self.active.push(ty);
        let normalized = self.normalize(ty, depth);
        self.active.pop();
        self.done.insert(ty, normalized);
        normalized
    }

    fn normalize(&mut self, ty: Ty, depth: u32) -> Ty {
        let (items, sigs) = (self.items, self.sigs);
        match ty.kind() {
            TyKind::Alias(alias) => {
                let args: Vec<Ty> = alias.args.iter().map(|&a| self.go(a, depth + 1)).collect();
                let alias = AliasTy {
                    args: Ty::list(&args),
                    ..alias
                };
                if let Some(p) = self.env.iter().find(|p| p.alias == alias) {
                    return self.go(p.ty, depth + 1);
                }
                if let TyKind::Alias(outer) = args[0].kind()
                    && let Some(p) = alias_projections(sigs, outer)
                        .into_iter()
                        .find(|p| p.alias == alias)
                {
                    return self.go(p.ty, depth + 1);
                }
                let goal = alias_predicate(alias);
                let impls = items.trait_impls.get(&alias.trait_).into_iter().flatten();
                for &imp in impls {
                    if let Some(impl_args) = match_impl(items, sigs, imp, goal)
                        && let Some(t) = impl_type(sigs, imp, alias.name)
                    {
                        return self.go(t.subst(&impl_args), depth + 1);
                    }
                }
                Ty::alias(alias)
            }
            TyKind::Tuple(list) => {
                let list: Vec<Ty> = list.iter().map(|&t| self.go(t, depth + 1)).collect();
                Ty::tuple(&list)
            }
            TyKind::Adt(adt, list) => {
                let list: Vec<Ty> = list.iter().map(|&t| self.go(t, depth + 1)).collect();
                Ty::adt(adt.strukt, adt.name, &list)
            }
            TyKind::Ref(m, t) => Ty::new(TyKind::Ref(m, self.go(t, depth + 1))),
            TyKind::RawPtr(m, t) => Ty::raw_ptr(m, self.go(t, depth + 1)),
            _ => ty,
        }
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
    if in_env(sigs, env, goal) {
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
        nested.copied().all(|p| {
            holds_at(
                items,
                sigs,
                env,
                normalize_predicate(items, sigs, p.subst(&args)),
                stack,
            )
        })
    });
    stack.pop();
    holds
}

/// `p` with the associated types of known types in it normalized.
fn normalize_predicate(items: &ItemTree, sigs: &Signatures, p: Predicate) -> Predicate {
    if !p.self_ty.has_aliases() && !p.trait_ref.args.iter().any(|a| a.has_aliases()) {
        return p;
    }
    let args: Vec<Ty> = p
        .trait_ref
        .args
        .iter()
        .map(|&a| normalize(items, sigs, &[], a))
        .collect();
    Predicate {
        self_ty: normalize(items, sigs, &[], p.self_ty),
        trait_ref: TraitRef {
            args: Ty::list(&args),
            ..p.trait_ref
        },
    }
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
            let Some(&copy) = items.lang_traits.get(&flux_intern::sym::copy) else {
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

/// The `impl Drop` block of `strukt`, if it has one.
pub fn drop_impl(items: &ItemTree, sigs: &Signatures, strukt: StructId) -> Option<ImplId> {
    let drop = *items.lang_traits.get(&flux_intern::sym::drop)?;
    items.trait_impls.get(&drop)?.iter().copied().find(|&imp| {
        matches!(sigs.impls.get(imp).map(|t| t.kind()), Some(TyKind::Adt(adt, _)) if adt.strukt == strukt)
    })
}

/// Whether dropping a `ty` runs code, given `env`, the elaborated bounds
/// in scope: it is, or has a part that is, a struct with a `Drop` impl,
/// or a type parameter that is not `Copy`, which may be one.
pub fn needs_drop(items: &ItemTree, sigs: &Signatures, env: &[Predicate], ty: Ty) -> bool {
    match ty.kind() {
        TyKind::Tuple(elements) => elements.iter().any(|&e| needs_drop(items, sigs, env, e)),
        TyKind::Adt(adt, args) => {
            drop_impl(items, sigs, adt.strukt).is_some()
                || sigs.structs[adt.strukt]
                    .iter()
                    .any(|f| needs_drop(items, sigs, env, f.subst(args)))
        }
        TyKind::Param(_) | TyKind::Alias(_) => !is_copy(items, sigs, env, ty),
        _ => false,
    }
}

/// Whether `function` is a destructor: `Drop::drop`, or an `impl Drop`
/// block's `drop`, which may not be called directly.
pub fn is_destructor(items: &ItemTree, function: FunctionId) -> bool {
    let data = &items.functions[function];
    let t = match data.owner {
        Some(Owner::Trait(t)) => Some(t),
        Some(Owner::Impl(imp)) => items.impls[imp].trait_,
        None => None,
    };
    t.and_then(|t| items.traits[t].lang)
        .is_some_and(|lang| lang == flux_intern::sym::drop)
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

/// The constant a path to `c` names once its generic arguments are
/// known: for a trait's, the `impl` block's for them, or the trait's
/// default; any other constant is itself.
pub fn resolve_const(items: &ItemTree, sigs: &Signatures, c: ConstInstance) -> ConstInstance {
    let args: Vec<Ty> = c
        .args
        .iter()
        .map(|&a| normalize(items, sigs, &[], a))
        .collect();
    let c = ConstInstance {
        args: Ty::list(&args),
        ..c
    };
    let data = &items.consts[c.konst];
    let (Some(Owner::Trait(t)), Some(name)) = (data.owner, data.name) else {
        return c;
    };
    let goal = Predicate {
        self_ty: args[0],
        trait_ref: TraitRef {
            trait_: t,
            name: items.traits[t].name,
            args: Ty::list(&args[1..]),
        },
    };
    for &imp in items.trait_impls.get(&t).into_iter().flatten() {
        let Some(impl_args) = match_impl(items, sigs, imp, goal) else {
            continue;
        };
        let found = items.impls[imp]
            .consts
            .iter()
            .copied()
            .find(|&k| items.consts[k].name == Some(name));
        return match found {
            Some(konst) => ConstInstance {
                konst,
                args: Ty::list(&impl_args),
            },
            None => c,
        };
    }
    c
}

/// Checks each trait's `impl` blocks against the trait: that they define
/// its required methods and no others, with its signatures; that they
/// are `unsafe` exactly when it is; that their types implement its
/// supertraits; and that no two of them overlap. Only the blocks from
/// index `checked` on are checked: `core`'s are checked by its tests,
/// not on every build.
pub fn check_impls(
    items: &ItemTree,
    sigs: &Signatures,
    diagnostics: &mut Vec<Diagnostic>,
    checked: u32,
) {
    // As in rustc, a trait with conflicting impls, or `Copy` with one
    // for a type that cannot be, has its impls checked no further.
    let mut skipped = check_overlap(items, sigs, diagnostics, checked);
    if !items
        .lang_traits
        .get(&flux_intern::sym::drop)
        .is_some_and(|d| skipped.contains(d))
    {
        check_drop_impls(items, sigs, diagnostics, checked);
    }
    if check_copy_impls(items, sigs, diagnostics, checked)
        && let Some(&copy) = items.lang_traits.get(&flux_intern::sym::copy)
    {
        skipped.push(copy);
    }
    for (id, imp) in items.impls.iter() {
        let Some(t) = imp.trait_ else {
            continue;
        };
        if id.index() < checked as usize || skipped.contains(&t) {
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
        let has_const = |name| data.konst(items, name).is_some();
        for &f in &imp.functions {
            let function = &items.functions[f];
            match data.function(items, function.name) {
                Some(required) => check_method(items, sigs, id, f, required, &mut error),
                None if has_type(function.name) || has_const(function.name) => error(
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
            } else if data.value(items, ty.name).is_some() {
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
        for &c in &imp.consts {
            let konst = &items.consts[c];
            let Some(name) = konst.name else {
                continue;
            };
            match data.konst(items, name) {
                Some(required) => {
                    // Its type, for the block's types, must be the trait's.
                    let (Some(&trait_ref), Some(&self_ty)) =
                        (sigs.impl_traits.get(id), sigs.impls.get(id))
                    else {
                        continue;
                    };
                    let args: Vec<Ty> = std::iter::once(self_ty)
                        .chain(trait_ref.args.iter().copied())
                        .collect();
                    let projections = sigs.impl_projections.get(id).map_or(&[][..], |p| p);
                    let expected =
                        normalize(items, sigs, projections, sigs.consts[required].subst(&args));
                    let found = normalize(items, sigs, projections, sigs.consts[c]);
                    if expected != found && expected != Ty::ERROR && found != Ty::ERROR {
                        error(
                            konst.ty_range,
                            format!(
                                "implemented const `{name}` has an incompatible type for trait"
                            ),
                        );
                    }
                }
                None if has_type(name) || data.function(items, name).is_some() => error(
                    konst.item_range,
                    format!(
                        "item `{name}` is an associated const, which doesn't match its trait `{}`",
                        data.name
                    ),
                ),
                None => error(
                    konst.item_range,
                    format!("const `{name}` is not a member of trait `{}`", data.name),
                ),
            }
        }
        // Required items not defined, in the trait's order.
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
        missing.extend(
            data.consts
                .iter()
                .map(|&c| &items.consts[c])
                .filter(|c| !c.has_body)
                .filter_map(|c| Some((c.item_range.start(), c.name?)))
                .filter(|&(_, name)| {
                    !imp.consts
                        .iter()
                        .any(|&d| items.consts[d].name == Some(name))
                }),
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
/// once per cycle, at the first trait's bound that leads into it. Only
/// cycles through the traits from `from` on are looked for: the traits
/// before cannot name them.
pub fn check_supertrait_cycles(
    items: &ItemTree,
    sigs: &Signatures,
    diagnostics: &mut Vec<Diagnostic>,
    from: ItemCounts,
) -> bool {
    let mut in_cycle: Vec<TraitId> = Vec::new();
    let mut found_any = false;
    for (start, data) in items.traits.iter().skip(from.traits as usize) {
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
/// such as `where S: Tr` for an `S` that does not implement `Tr`, on the
/// functions and `impl` blocks from `from` on.
pub fn check_trivial_bounds(
    items: &ItemTree,
    sigs: &Signatures,
    diagnostics: &mut Vec<Diagnostic>,
    from: ItemCounts,
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
    for (id, f) in items.functions.iter().skip(from.functions as usize) {
        let predicates = sigs.fn_predicates.get(id).map_or(&[][..], |p| p);
        if predicates.len() == f.generics.predicates.len() {
            check(&f.generics, predicates, f.loc.file);
        }
    }
    for (id, imp) in items.impls.iter().skip(from.impls as usize) {
        let predicates = sigs.impl_predicates.get(id).map_or(&[][..], |p| p);
        if predicates.len() == imp.generics.predicates.len() {
            check(&imp.generics, predicates, imp.file);
        }
    }
    for (id, s) in items.structs.iter().skip(from.structs as usize) {
        let predicates = sigs.struct_predicates.get(id).map_or(&[][..], |p| p);
        if predicates.len() == s.generics.predicates.len() {
            check(&s.generics, predicates, s.loc.file);
        }
    }
}

/// Checks that the types in items' signatures meet the bounds of the
/// structs they name, given the bounds in scope where they are written:
/// `W<u8>` with `struct W<T: Tr>` needs `u8: Tr` (E0277). As in rustc, a
/// struct's bounds are not assumed where it is named, so `fn f<T>(w:
/// W<T>)` needs `T: Tr` too, and an associated type needs its trait:
/// `<T as Tr>::Out` needs `T: Tr`. The types in bounds are checked too.
/// Each unmet bound is reported once per item, at the innermost type
/// that needs it where it is first met, as rustc does.
pub fn check_well_formed(
    items: &ItemTree,
    sigs: &Signatures,
    from: ItemCounts,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let scope = |predicates: Vec<Predicate>, mut projections: Vec<Projection>| {
        let env = elaborate(sigs, &predicates);
        projections.extend(implied_projections(sigs, &env));
        (env, projections)
    };
    // Without bounds on structs, only the items that name associated
    // types have anything to check.
    let struct_bounds = sigs.struct_predicates.iter().next().is_some()
        || sigs.struct_projections.iter().next().is_some();
    let aliased = |tys: &[Ty]| tys.iter().any(|t| t.has_aliases());
    let aliased_bounds = |predicates: &[Predicate], declared: &[Projection]| {
        predicates
            .iter()
            .any(|p| p.self_ty.has_aliases() || aliased(p.trait_ref.args))
            || declared
                .iter()
                .any(|q| q.ty.has_aliases() || aliased(q.alias.args))
    };
    let aliased_owner = |owner: Option<Owner>| {
        let (predicates, declared) = match owner {
            Some(Owner::Impl(imp)) => (
                sigs.impl_predicates.get(imp),
                sigs.impl_projections.get(imp),
            ),
            Some(Owner::Trait(t)) => (sigs.trait_predicates.get(t), sigs.trait_projections.get(t)),
            None => return false,
        };
        aliased_bounds(
            predicates.map_or(&[][..], |p| p),
            declared.map_or(&[][..], |p| p),
        )
    };
    for (id, s) in items.structs.iter().skip(from.structs as usize) {
        let own = sigs.struct_predicates.get(id).map_or(&[][..], |p| p);
        let declared = sigs.struct_projections.get(id).map_or(&[][..], |p| p);
        if !struct_bounds && !aliased(sigs.structs[id]) && !aliased_bounds(own, declared) {
            continue;
        }
        let (env, projections) = scope(own.to_vec(), declared.to_vec());
        let wf = WfScope {
            env: &env,
            projections: &projections,
            aliases: true,
        };
        let span = TextRange::new(
            s.def_range.start(),
            s.def_range.end().max(s.generics.range.end()),
        );
        let mut check = WfCheck::new(items, sigs, s.loc.file, &wf, span, diagnostics);
        for (field, &ty) in crate::all_fields(s).zip(sigs.structs[id]) {
            check.ty(ty, Some(&field.ty), WfRange::at(field.range));
        }
        check.bounds(&s.generics, own, declared);
    }
    for (id, imp) in items.impls.iter().skip(from.impls as usize) {
        let own = sigs.impl_predicates.get(id).map_or(&[][..], |p| p);
        let declared = sigs.impl_projections.get(id).map_or(&[][..], |p| p);
        let types = sigs.impl_types.get(id).map_or(&[][..], |t| t);
        let trait_args = sigs.impl_traits.get(id).map_or(&[][..], |t| t.args);
        if !struct_bounds
            && !sigs.impls[id].has_aliases()
            && !aliased(trait_args)
            && !types.iter().any(|&(_, t)| t.has_aliases())
            && !aliased_bounds(own, declared)
        {
            continue;
        }
        // rustc checks no more of a block with a parameter its header does
        // not constrain (E0207).
        let constrained = (0..imp.generics.params.len() as u32).all(|i| {
            crate::mentions_param(sigs.impls[id], i)
                || trait_args.iter().any(|&a| crate::mentions_param(a, i))
        });
        if !constrained {
            continue;
        }
        let (env, projections) = scope(own.to_vec(), declared.to_vec());
        let wf = WfScope {
            env: &env,
            projections: &projections,
            aliases: true,
        };
        let mut check = WfCheck::new(items, sigs, imp.file, &wf, imp.def_range, diagnostics);
        // rustc reports the bounds of structs in a header's types at the
        // whole type, and associated types where they are.
        let whole = |range| WfRange {
            whole: Some(range),
            ..WfRange::at(range)
        };
        check.ty(sigs.impls[id], Some(&imp.self_ty), whole(imp.self_range));
        if let (Some(t), Some(bound)) = (sigs.impl_traits.get(id), &imp.trait_ref) {
            let written = bound.path.args.last().and_then(Option::as_ref);
            for (i, &arg) in t.args.iter().enumerate() {
                let part = written.and_then(|a| Some((a.types.get(i)?, a.ranges[i])));
                let at = WfRange {
                    whole: Some(bound.range),
                    ..WfRange::at(part.map_or(bound.range, |(_, r)| r))
                };
                check.ty(arg, part.map(|(t, _)| t), at);
            }
        }
        for (written, &(_, ty)) in imp.types.iter().zip(types) {
            check.ty(ty, Some(&written.ty), WfRange::at(written.ty_range));
        }
        check.bounds(&imp.generics, own, declared);
    }
    for (id, t) in items.traits.iter().skip(from.traits as usize) {
        let own = sigs.trait_predicates.get(id).map_or(&[][..], |p| p);
        let declared = sigs.trait_projections.get(id).map_or(&[][..], |p| p);
        if !struct_bounds && !aliased_bounds(own, declared) {
            continue;
        }
        let (env, projections) = scope(
            owner_predicates(items, sigs, Some(Owner::Trait(id))),
            Vec::new(),
        );
        let wf = WfScope {
            env: &env,
            projections: &projections,
            aliases: true,
        };
        let span = TextRange::new(
            t.def_range.start(),
            t.def_range.end().max(t.generics.range.end()),
        );
        WfCheck::new(items, sigs, t.loc.file, &wf, span, diagnostics).bounds(
            &t.generics,
            own,
            declared,
        );
    }
    for (id, konst) in items.consts.iter().skip(from.consts as usize) {
        if !struct_bounds && !sigs.consts[id].has_aliases() && !aliased_owner(konst.owner) {
            continue;
        }
        let (env, projections) = scope(owner_predicates(items, sigs, konst.owner), Vec::new());
        let wf = WfScope {
            env: &env,
            projections: &projections,
            aliases: true,
        };
        let mut check = WfCheck::new(
            items,
            sigs,
            konst.loc.file,
            &wf,
            konst.def_range,
            diagnostics,
        );
        check.ty(
            sigs.consts[id],
            Some(&konst.ty),
            WfRange::at(konst.ty_range),
        );
        check.owner_bounds(konst.owner);
    }
    for (id, f) in items.functions.iter().skip(from.functions as usize) {
        let sig = &sigs.fns[id];
        let own = sigs.fn_predicates.get(id).map_or(&[][..], |p| p);
        let declared = sigs.fn_projections.get(id).map_or(&[][..], |p| p);
        if !struct_bounds
            && !aliased(&sig.params)
            && !sig.ret.has_aliases()
            && !aliased_bounds(own, declared)
            && !aliased_owner(f.owner)
        {
            continue;
        }
        let env = elaborate(sigs, &predicates_of(items, sigs, id));
        let projections = projections_of(items, sigs, id);
        let wf = WfScope {
            env: &env,
            projections: &projections,
            aliases: true,
        };
        let mut check = WfCheck::new(items, sigs, f.loc.file, &wf, f.sig_range, diagnostics);
        for (i, (param, &ty)) in f.params.iter().zip(&sig.params).enumerate() {
            // `self` in `&self`, as rustc points, but the whole of it for
            // what an associated type is.
            let range = if i == 0 && f.has_self {
                WfRange {
                    bound: TextRange::new(param.range.end() - 4, param.range.end()),
                    projection: param.range,
                    whole: None,
                }
            } else {
                WfRange::at(param.range)
            };
            check.ty(ty, Some(&param.ty), range);
        }
        check.ty(sig.ret, Some(&f.ret), WfRange::at(f.ret_range));
        check.owner_bounds(f.owner);
        check.bounds(&f.generics, own, declared);
        if f.has_body {
            let types = f.params.iter().map(|p| p.range).zip(sig.params.iter());
            check.normalized(types.chain([(f.ret_range, &sig.ret)]));
        }
        // A trait's method in an `impl` block: its trait's signature, for
        // the block's types, at the whole signature, given the method's
        // bounds in the trait as well.
        let Some(imp) = items.impl_of(id) else {
            continue;
        };
        let (Some(t), Some(&trait_ref)) = (items.impls[imp].trait_, sigs.impl_traits.get(imp))
        else {
            continue;
        };
        let Some(required) = items.traits[t].function(items, f.name) else {
            continue;
        };
        let mut args = vec![sigs.impls[imp]];
        args.extend_from_slice(trait_ref.args);
        let first_own = items.impls[imp].generics.len();
        args.extend(
            f.generics
                .params
                .iter()
                .enumerate()
                .map(|(j, p)| Ty::param((first_own + j) as u32, p.name)),
        );
        if args.len()
            != owner_param_count(items, Some(Owner::Trait(t)))
                + items.functions[required].generics.len()
        {
            continue;
        }
        let mut assumed = predicates_of(items, sigs, id);
        let theirs = sigs.fn_predicates.get(required).into_iter().flatten();
        assumed.extend(theirs.map(|p| p.subst(&args)));
        let env = elaborate(sigs, &assumed);
        let wf = WfScope {
            env: &env,
            projections: &projections,
            aliases: false,
        };
        let required = sigs.fns[required].subst(&args);
        let mut check = WfCheck::new(items, sigs, f.loc.file, &wf, f.def_range, diagnostics);
        for &ty in required.params.iter().chain([&required.ret]) {
            check.ty(ty, None, WfRange::at(f.def_range));
        }
    }
}

/// The well-formedness checks of one item's signature, which report
/// each unmet bound once, as rustc does.
struct WfCheck<'a, 'd> {
    items: &'a ItemTree,
    sigs: &'a Signatures,
    file: FileId,
    scope: &'a WfScope<'a>,
    /// Where an associated type in the item's bounds whose trait does
    /// not hold is reported: the item's signature, or its name.
    span: TextRange,
    diagnostics: &'d mut Vec<Diagnostic>,
    /// The unmet bounds reported.
    reported: Vec<Predicate>,
}

impl<'a, 'd> WfCheck<'a, 'd> {
    fn new(
        items: &'a ItemTree,
        sigs: &'a Signatures,
        file: FileId,
        scope: &'a WfScope<'a>,
        span: TextRange,
        diagnostics: &'d mut Vec<Diagnostic>,
    ) -> WfCheck<'a, 'd> {
        WfCheck {
            items,
            sigs,
            file,
            scope,
            span,
            diagnostics,
            reported: Vec::new(),
        }
    }

    fn report(&mut self, range: TextRange, message: String) {
        let d = Diagnostic {
            file: self.file,
            range,
            message,
        };
        if !self.diagnostics.contains(&d) {
            self.diagnostics.push(d);
        }
    }

    /// Reports how `ty`, written as `written`, fails the bounds of the
    /// structs and traits of associated types it names.
    fn ty(&mut self, ty: Ty, written: Option<&TypeRef>, range: WfRange) {
        let mut found = Vec::new();
        unmet_struct_bounds(
            self.items,
            self.sigs,
            self.scope,
            ty,
            written,
            range,
            &mut self.reported,
            &mut found,
        );
        for (range, message) in found {
            self.report(range, message);
        }
    }

    /// [`Self::ty`] for the types in bounds, `generics` as written and
    /// `predicates` and `declared` as resolved: each at the bound, or
    /// the binding, that has it.
    fn bounds(&mut self, generics: &Generics, predicates: &[Predicate], declared: &[Projection]) {
        self.bound_aliases(predicates, declared);
        if predicates.len() != generics.predicates.len() {
            return;
        }
        for (written, p) in generics.predicates.iter().zip(predicates) {
            let at = WfRange::at(written.bound.range);
            for ty in std::iter::once(p.self_ty).chain(p.trait_ref.args.iter().copied()) {
                self.ty(ty, None, at);
            }
            let bindings = written.bound.path.args.last().into_iter().flatten();
            for b in bindings.flat_map(|a| a.bindings.iter()) {
                let q = declared.iter().find(|q| {
                    q.alias.name == b.name
                        && q.alias.trait_ == p.trait_ref.trait_
                        && q.alias.args[0] == p.self_ty
                });
                if let Some(q) = q {
                    self.ty(q.ty, Some(&b.ty), WfRange::at(b.range()));
                }
            }
        }
    }

    /// The associated types in the bounds of `owner`, the `impl` block or
    /// trait of an associated item, which rustc reports at the item.
    fn owner_bounds(&mut self, owner: Option<Owner>) {
        let (predicates, declared) = match owner {
            Some(Owner::Impl(imp)) => (
                self.sigs.impl_predicates.get(imp),
                self.sigs.impl_projections.get(imp),
            ),
            Some(Owner::Trait(t)) => (
                self.sigs.trait_predicates.get(t),
                self.sigs.trait_projections.get(t),
            ),
            None => return,
        };
        self.bound_aliases(
            predicates.map_or(&[][..], |p| p),
            declared.map_or(&[][..], |p| p),
        );
    }

    /// Reports the associated types in `predicates` and `declared` whose
    /// traits do not hold, at the item: rustc normalizes an item's bounds
    /// there.
    fn bound_aliases(&mut self, predicates: &[Predicate], declared: &[Projection]) {
        if !self.scope.aliases {
            return;
        }
        let mut aliases = Vec::new();
        for p in predicates {
            collect_aliases(p.self_ty, &mut aliases);
            for &a in p.trait_ref.args {
                collect_aliases(a, &mut aliases);
            }
        }
        for q in declared {
            for &a in q.alias.args {
                collect_aliases(a, &mut aliases);
            }
            collect_aliases(q.ty, &mut aliases);
        }
        for alias in aliases {
            let goal = normalize_predicate(self.items, self.sigs, alias_predicate(alias));
            if holds(self.items, self.sigs, self.scope.env, goal) {
                continue;
            }
            let leaf = unmet_leaf(self.items, self.sigs, self.scope.env, goal, 0);
            if !self.reported.contains(&leaf) {
                self.reported.push(leaf);
                let message = unmet(self.items, leaf);
                self.report(self.span, message);
            }
        }
    }

    /// What type checking a function's body adds, which normalizes the
    /// types of its signature, `types` with where each is written: an
    /// associated type whose trait does not hold, at the whole type, each
    /// associated type once, unless that is where its check reported it.
    fn normalized<'t>(&mut self, types: impl Iterator<Item = (TextRange, &'t Ty)>) {
        if !self.scope.aliases {
            return;
        }
        let mut seen = Vec::new();
        for (range, &ty) in types {
            let mut aliases = Vec::new();
            collect_aliases(ty, &mut aliases);
            for alias in aliases {
                if seen.contains(&alias) {
                    continue;
                }
                seen.push(alias);
                let goal = normalize_predicate(self.items, self.sigs, alias_predicate(alias));
                if !holds(self.items, self.sigs, self.scope.env, goal) {
                    let leaf = unmet_leaf(self.items, self.sigs, self.scope.env, goal, 0);
                    let message = unmet(self.items, leaf);
                    self.report(range, message);
                }
            }
        }
    }
}

/// Whether the header of `impl` block `imp` names an associated type
/// whose trait holds for no types the block may be for, given its
/// bounds: then it overlaps no other block, as rustc finds.
fn broken_header(items: &ItemTree, sigs: &Signatures, imp: ImplId) -> bool {
    let self_ty = sigs.impls.get(imp).copied().unwrap_or(Ty::ERROR);
    let trait_args = sigs.impl_traits.get(imp).map_or(&[][..], |t| t.args);
    let header = || std::iter::once(self_ty).chain(trait_args.iter().copied());
    if !header().any(|t| t.has_aliases()) {
        return false;
    }
    let own = sigs.impl_predicates.get(imp).map_or(&[][..], |p| p);
    let env = elaborate(sigs, own);
    let mut aliases = Vec::new();
    header().for_each(|t| collect_aliases(t, &mut aliases));
    aliases.into_iter().any(|alias| {
        let goal = normalize_predicate(items, sigs, alias_predicate(alias));
        if holds(items, sigs, &env, goal) {
            return false;
        }
        let impls = items.trait_impls.get(&goal.trait_ref.trait_);
        !impls.into_iter().flatten().any(|&other| {
            sigs.impls
                .get(other)
                .is_some_and(|&ty| crate::overlap(ty, goal.self_ty))
        })
    })
}

/// Whether `ty` has an associated type whose trait does not hold given
/// `env`.
fn broken_alias(items: &ItemTree, sigs: &Signatures, env: &[Predicate], ty: Ty) -> bool {
    if !ty.has_aliases() {
        return false;
    }
    let mut aliases = Vec::new();
    collect_aliases(ty, &mut aliases);
    aliases.into_iter().any(|alias| {
        let goal = normalize_predicate(items, sigs, alias_predicate(alias));
        !holds(items, sigs, env, goal)
    })
}

/// The associated types in `ty`, those in their arguments first.
fn collect_aliases(ty: Ty, out: &mut Vec<AliasTy>) {
    match ty.kind() {
        TyKind::Alias(alias) => {
            for &a in alias.args {
                collect_aliases(a, out);
            }
            out.push(alias);
        }
        TyKind::Adt(_, list) | TyKind::Tuple(list) => {
            list.iter().for_each(|&t| collect_aliases(t, out));
        }
        TyKind::Ref(_, t) | TyKind::RawPtr(_, t) => collect_aliases(t, out),
        _ => {}
    }
}

/// What a well-formedness check may assume: the elaborated bounds in
/// scope, and what they say associated types are.
pub struct WfScope<'a> {
    pub env: &'a [Predicate],
    pub projections: &'a [Projection],
    /// Whether associated types need their traits: not where a trait's
    /// signature is checked for an `impl` block's types.
    pub aliases: bool,
}

/// Where a type's unmet bounds are reported: E0277s at `bound`, E0271s
/// at `projection` (rustc's spans differ for `&self`).
#[derive(Clone, Copy)]
struct WfRange {
    bound: TextRange,
    projection: TextRange,
    /// Where the bounds of structs are reported instead, if not at the
    /// type itself: an `impl` block's header's whole type.
    whole: Option<TextRange>,
}

impl WfRange {
    fn at(range: TextRange) -> WfRange {
        WfRange {
            bound: range,
            projection: range,
            whole: None,
        }
    }

    /// The range of a type inside this one, written at `range`.
    fn inner(self, range: TextRange) -> WfRange {
        WfRange {
            whole: self.whole,
            ..WfRange::at(range)
        }
    }
}

/// The range of `ty` as written, if it is a path: from its first segment
/// (or qualified self) to its last segment or generic arguments.
fn written_range(ty: &TypeRef) -> Option<TextRange> {
    let TypeRef::Path(path) = ty else {
        return None;
    };
    let start = match &path.qself {
        Some(qself) => qself.range.start(),
        None => path.anchor_ranges.first().or(path.ranges.first())?.start(),
    };
    let end = match path.args.last() {
        Some(Some(args)) => args.range.end(),
        _ => path.ranges.last()?.end(),
    };
    Some(TextRange::new(start, end))
}

/// The parts of `written` that stand for the types inside `ty`: its
/// generic arguments, tuple elements or pointee, if written alike.
fn written_parts(written: Option<&TypeRef>) -> Vec<&TypeRef> {
    match written {
        Some(TypeRef::Path(path)) => match path.args.last() {
            Some(Some(args)) => args.types.iter().collect(),
            _ => Vec::new(),
        },
        Some(TypeRef::Tuple(elements)) => elements.iter().collect(),
        Some(TypeRef::Ref { pointee, .. } | TypeRef::Ptr { pointee, .. }) => vec![&**pointee],
        _ => Vec::new(),
    }
}

/// How `ty`, written as `written` at `range` if known, fails the bounds
/// of the structs it names, given `scope`: each unmet bound once, as its
/// innermost cause (`S: Copy` for `S: Tr` with `impl<T: Copy> Tr for
/// T`), at the innermost type that needs it, and what associated types
/// are not what bounds say.
#[allow(clippy::too_many_arguments)]
fn unmet_struct_bounds(
    items: &ItemTree,
    sigs: &Signatures,
    scope: &WfScope<'_>,
    ty: Ty,
    written: Option<&TypeRef>,
    range: WfRange,
    reported: &mut Vec<Predicate>,
    out: &mut Vec<(TextRange, String)>,
) {
    let parts = written_parts(written);
    let inner = |i: usize| {
        let part = parts.get(i).copied();
        let at = part
            .and_then(written_range)
            .map_or(range, |r| range.inner(r));
        (part, at)
    };
    match ty.kind() {
        TyKind::Adt(adt, args) => {
            for (i, &arg) in args.iter().enumerate() {
                let (part, at) = inner(i);
                unmet_struct_bounds(items, sigs, scope, arg, part, at, reported, out);
            }
            let normalized = |t: Ty| normalize(items, sigs, scope.projections, t);
            let preds = sigs.struct_predicates.get(adt.strukt).into_iter().flatten();
            for p in preds {
                let p = p.subst(args);
                // An associated type whose trait does not hold is
                // reported itself, as an error type.
                if scope.aliases
                    && std::iter::once(p.self_ty)
                        .chain(p.trait_ref.args.iter().copied())
                        .any(|t| broken_alias(items, sigs, scope.env, t))
                {
                    continue;
                }
                let p = normalize_predicate(items, sigs, p);
                let p = Predicate {
                    self_ty: normalized(p.self_ty),
                    trait_ref: TraitRef {
                        args: Ty::list(
                            &p.trait_ref
                                .args
                                .iter()
                                .map(|&a| normalized(a))
                                .collect::<Vec<_>>(),
                        ),
                        ..p.trait_ref
                    },
                };
                if holds(items, sigs, scope.env, p) {
                    continue;
                }
                let leaf = unmet_leaf(items, sigs, scope.env, p, 0);
                if !reported.contains(&leaf) {
                    reported.push(leaf);
                    out.push((range.whole.unwrap_or(range.bound), unmet(items, leaf)));
                }
            }
            for q in sigs
                .struct_projections
                .get(adt.strukt)
                .into_iter()
                .flatten()
            {
                let q = q.subst(args);
                let alias = AliasTy {
                    args: Ty::list(
                        &q.alias
                            .args
                            .iter()
                            .map(|&a| normalized(a))
                            .collect::<Vec<_>>(),
                    ),
                    ..q.alias
                };
                // Without the trait, there is no associated type (E0277).
                let goal = Predicate {
                    self_ty: alias.args[0],
                    trait_ref: TraitRef {
                        trait_: alias.trait_,
                        name: alias.trait_name,
                        args: Ty::list(&alias.args[1..]),
                    },
                };
                if !holds(items, sigs, scope.env, goal) {
                    continue;
                }
                let found = normalized(Ty::alias(alias));
                let expected = normalized(q.ty);
                if found != expected && found != Ty::ERROR && expected != Ty::ERROR {
                    out.push((
                        range.whole.unwrap_or(range.projection),
                        format!(
                            "type mismatch resolving `{} == {expected}`",
                            Ty::alias(alias)
                        ),
                    ));
                }
            }
        }
        // An associated type needs its trait: `<T as Tr>::Out` needs
        // `T: Tr`, at the type, as rustc points.
        TyKind::Alias(alias) if scope.aliases => {
            let qself = match written {
                Some(TypeRef::Path(path)) => path.qself.as_deref(),
                _ => None,
            };
            for (i, &arg) in alias.args.iter().enumerate() {
                // `T` and the trait's arguments, as written.
                let (part, at) = match qself {
                    Some(q) if i == 0 => (Some(&q.ty), range.inner(q.ty_range)),
                    Some(q) => {
                        let args = q.trait_.as_ref().and_then(|t| t.args.last()?.as_ref());
                        match args {
                            Some(a) if i - 1 < a.types.len() => {
                                (Some(&a.types[i - 1]), range.inner(a.ranges[i - 1]))
                            }
                            _ => (None, range),
                        }
                    }
                    None => (None, range),
                };
                unmet_struct_bounds(items, sigs, scope, arg, part, at, reported, out);
            }
            let normalized = |t: Ty| normalize(items, sigs, scope.projections, t);
            let goal = alias_predicate(AliasTy {
                args: Ty::list(
                    &alias
                        .args
                        .iter()
                        .map(|&a| normalized(a))
                        .collect::<Vec<_>>(),
                ),
                ..alias
            });
            let goal = normalize_predicate(items, sigs, goal);
            if !holds(items, sigs, scope.env, goal) {
                let leaf = unmet_leaf(items, sigs, scope.env, goal, 0);
                if !reported.contains(&leaf) {
                    reported.push(leaf);
                    out.push((range.bound, unmet(items, leaf)));
                }
            }
        }
        TyKind::Tuple(elements) => {
            for (i, &e) in elements.iter().enumerate() {
                let (part, at) = inner(i);
                unmet_struct_bounds(items, sigs, scope, e, part, at, reported, out);
            }
        }
        TyKind::Ref(_, t) | TyKind::RawPtr(_, t) => {
            let (part, at) = inner(0);
            unmet_struct_bounds(items, sigs, scope, t, part, at, reported, out);
        }
        _ => {}
    }
}

/// What makes `goal`, which does not hold given `env`, fail: through the
/// one `impl` block whose header matches it, the first of its bounds
/// that fails, as rustc reports; else `goal` itself.
pub fn unmet_leaf(
    items: &ItemTree,
    sigs: &Signatures,
    env: &[Predicate],
    goal: Predicate,
    depth: u32,
) -> Predicate {
    if depth > 32 {
        return goal;
    }
    let impls = items
        .trait_impls
        .get(&goal.trait_ref.trait_)
        .into_iter()
        .flatten();
    let mut matching = impls.filter_map(|&imp| Some((imp, match_impl(items, sigs, imp, goal)?)));
    let (Some((imp, args)), None) = (matching.next(), matching.next()) else {
        return goal;
    };
    for p in sigs.impl_predicates.get(imp).into_iter().flatten() {
        let p = normalize_predicate(items, sigs, p.subst(&args));
        if !holds(items, sigs, env, p) {
            return unmet_leaf(items, sigs, env, p, depth + 1);
        }
    }
    goal
}

/// Reports `impl` blocks of one trait for types that may be the same.
/// Reports `impl` blocks of a trait that overlap an earlier one (E0119);
/// gives the traits that have any.
fn check_overlap(
    items: &ItemTree,
    sigs: &Signatures,
    diagnostics: &mut Vec<Diagnostic>,
    checked: u32,
) -> Vec<TraitId> {
    let mut conflicted = Vec::new();
    let mut traits: Vec<(&TraitId, &Vec<ImplId>)> = items.trait_impls.iter().collect();
    traits.sort_by_key(|(t, _)| t.index());
    for (_, impls) in traits {
        for (i, &later) in impls.iter().enumerate() {
            if later.index() < checked as usize {
                continue;
            }
            let (Some(&b), Some(b_ref)) = (sigs.impls.get(later), sigs.impl_traits.get(later))
            else {
                continue;
            };
            // A type that did not resolve, or names an associated type
            // whose trait does not hold, was reported already.
            if b == Ty::ERROR || broken_header(items, sigs, later) {
                continue;
            }
            let earlier = impls[..i].iter().find(|&&a| {
                let (Some(&a_ty), Some(a_ref)) = (sigs.impls.get(a), sigs.impl_traits.get(a))
                else {
                    return false;
                };
                a_ty != Ty::ERROR
                    && !broken_header(items, sigs, a)
                    && crate::overlap(a_ty, b)
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
    checked: u32,
) -> bool {
    let Some(&copy) = items.lang_traits.get(&flux_intern::sym::copy) else {
        return false;
    };
    let mut any = false;
    for &id in items.trait_impls.get(&copy).into_iter().flatten() {
        let imp = &items.impls[id];
        if id.index() < checked as usize {
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
        } else if let TyKind::Adt(adt, _) = self_ty.kind()
            && drop_impl(items, sigs, adt.strukt).is_some()
        {
            diagnostics.push(Diagnostic {
                file: imp.file,
                range: imp.self_range,
                message: "the trait `Copy` cannot be implemented for this type; the type has a destructor".into(),
            });
            any = true;
        }
    }
    any
}

/// `p`'s trait as rustc writes it, without the trailing arguments that
/// are their defaults: `PartialEq`, not `PartialEq<T>`, for `T: PartialEq`.
fn written_trait_ref(sigs: &Signatures, p: Predicate) -> String {
    let defaults = sigs
        .traits
        .get(p.trait_ref.trait_)
        .map_or(&[][..], |t| &t.defaults);
    let args = p.trait_ref.args;
    let mut len = args.len();
    while len > 0 {
        let before: Vec<Ty> = std::iter::once(p.self_ty)
            .chain(args[..len - 1].iter().copied())
            .collect();
        match defaults.get(len - 1).copied().flatten() {
            Some(default) if default.subst(&before) == args[len - 1] => len -= 1,
            _ => break,
        }
    }
    TraitRef {
        args: Ty::list(&args[..len]),
        ..p.trait_ref
    }
    .to_string()
}

/// Reports `Drop` impls for anything but a struct (E0120), for a struct
/// with arguments other than distinct type parameters (E0366), or with
/// bounds the struct does not have (E0367): a value is dropped the same
/// way whatever its type's arguments are.
fn check_drop_impls(
    items: &ItemTree,
    sigs: &Signatures,
    diagnostics: &mut Vec<Diagnostic>,
    checked: u32,
) {
    let Some(&drop) = items.lang_traits.get(&flux_intern::sym::drop) else {
        return;
    };
    for &id in items.trait_impls.get(&drop).into_iter().flatten() {
        let imp = &items.impls[id];
        if id.index() < checked as usize {
            continue;
        }
        let Some(&self_ty) = sigs.impls.get(id) else {
            continue;
        };
        let mut error = |range, message: &str| {
            diagnostics.push(Diagnostic {
                file: imp.file,
                range,
                message: message.into(),
            });
        };
        let TyKind::Adt(adt, args) = self_ty.kind() else {
            if self_ty != Ty::ERROR {
                error(
                    imp.self_range,
                    "the `Drop` trait may only be implemented for local structs, enums, and unions",
                );
            }
            continue;
        };
        let mut seen = Vec::new();
        let specialized = args.iter().any(|&a| match a.kind() {
            TyKind::Param(param) if !seen.contains(&param.index) => {
                seen.push(param.index);
                false
            }
            _ => true,
        });
        if specialized {
            error(imp.def_range, "`Drop` impls cannot be specialized");
            continue;
        }
        // Each bound must be one of the struct's, or follow from them.
        let struct_env = elaborate(
            sigs,
            sigs.struct_predicates
                .get(adt.strukt)
                .map_or(&[][..], |p| p),
        );
        // The impl's parameters as the struct's: `args[i]` is the impl's
        // parameter for the struct's `i`th.
        let mut to_struct = vec![Ty::ERROR; imp.generics.len()];
        let names = &items.structs[adt.strukt].generics.params;
        for (i, &a) in args.iter().enumerate() {
            if let TyKind::Param(param) = a.kind() {
                to_struct[param.index as usize] = Ty::param(i as u32, names[i].name);
            }
        }
        let predicates = sigs.impl_predicates.get(id).map_or(&[][..], |p| p);
        if predicates.len() == imp.generics.predicates.len() {
            for (written, &p) in imp.generics.predicates.iter().zip(predicates) {
                // Bounds that hold anyway: `T: Sized`, `i32: Copy`.
                let sized = items.traits[p.trait_ref.trait_].lang == Some(flux_intern::sym::sized);
                let has_params =
                    p.self_ty.has_params() || p.trait_ref.args.iter().any(|a| a.has_params());
                if sized
                    || !has_params && holds(items, sigs, &[], p)
                    || has_params && holds(items, sigs, &struct_env, p.subst(&to_struct))
                {
                    continue;
                }
                let p = format!("{}: {}", p.self_ty, written_trait_ref(sigs, p));
                error(
                    written.bound.range,
                    &format!(
                        "`Drop` impl requires `{p}` but the struct it is implemented for does not"
                    ),
                );
            }
            // So must what they say associated types are.
            let declared = sigs.impl_projections.get(id).map_or(&[][..], |p| p);
            let mut theirs = sigs
                .struct_projections
                .get(adt.strukt)
                .map_or(Vec::new(), |p| p.to_vec());
            theirs.extend(implied_projections(sigs, &struct_env));
            for (written, p) in imp.generics.predicates.iter().zip(predicates) {
                let bindings = written.bound.path.args.last().into_iter().flatten();
                for b in bindings.flat_map(|a| a.bindings.iter()) {
                    let q = declared.iter().find(|q| {
                        q.alias.name == b.name
                            && q.alias.trait_ == p.trait_ref.trait_
                            && q.alias.args[0] == p.self_ty
                    });
                    let Some(&q) = q else {
                        continue;
                    };
                    if theirs.contains(&q.subst(&to_struct)) {
                        continue;
                    }
                    error(
                        b.range(),
                        &format!(
                            "`Drop` impl requires `{} == {}` but the struct it is implemented for does not",
                            Ty::alias(q.alias),
                            q.ty
                        ),
                    );
                }
            }
        }
    }
}
