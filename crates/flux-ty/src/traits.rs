//! Traits outside of bodies: the bounds of items resolved as predicates,
//! and the checks rustc makes of `impl` blocks, supertraits, overlapping
//! blocks and well-formedness. Whether a predicate holds, and which
//! function a call of a trait's method calls, is `flux_solve::traits`'s.

use flux_hir::{
    Bound, Diagnostic, FileId, FunctionId, Generics, ImplId, ItemCounts, ItemTree, ModuleId, Owner,
    TraitId, TypeRef,
};
use flux_intern::{Symbol, kw};
use flux_syntax::TextRange;

use crate::regions::{ImplRegions, Outlives, Region, StructRegions};
use crate::ty::{AliasTy, Ty, TyKind};
use crate::{Signatures, TypeScope, resolve_type, wrong_arg_count};

use flux_solve::traits::{
    SignatureImplProof, alias_of, alias_predicate, auto_trait_requirements,
    caller_owner_projections, caller_projections_of, caller_supertrait_projections, drop_impl,
    elaborate, family_requirements, has_error, holds, impl_type, implied_projections,
    instantiate_impl_type, is_auto_trait, is_copy, is_rigid, match_impl, match_impl_sized,
    normalize, normalize_predicate, normalize_projection_rhs, owner_param_count, owner_predicates,
    predicates_of, projections_of, select_impl, signature_impl_evidence, trait_self_predicate,
};
pub use flux_ty_ir::traits::{
    AssociatedTypeSig, Bounds, Predicate, Projection, TraitDefaults, TraitRef, TraitSig,
};

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
    let scope = TypeScope {
        in_trait_bound: true,
        ..scope
    };
    let data = &items.traits[t];
    let path = &bound.path;
    let last = path.segments.len().saturating_sub(1);
    let at = path.range(last, bound.range);
    let want = data.generics.len();
    let written = path.args(last);
    // `Fn(A) -> B` is how stable Rust names the `Fn`-family traits, and
    // the only traits it may name so.
    let is_fn = crate::fn_trait_kind(items, t).is_some();
    match written {
        Some(w) if w.parenthesized && !is_fn => {
            errors.push((
                TextRange::new(at.start(), w.range.end()),
                "parenthetical notation is only stable when used with `Fn`-family traits".into(),
            ));
            return None;
        }
        // `core` itself may write them out.
        Some(w) if !w.parenthesized && is_fn && !items.is_core(module) => {
            let range = TextRange::new(at.start(), w.range.end());
            errors.push((
                range,
                "the precise format of `Fn`-family traits' type parameters is subject to change"
                    .into(),
            ));
            return None;
        }
        _ => {}
    }
    let mut args: Vec<Ty> = written.map_or(Vec::new(), |written| {
        crate::resolve_generic_args(items, module, &data.generics.params, written, scope, errors)
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
        // An associated type of a supertrait with the same parameters:
        // `Output` of `FnOnce` in `Fn(A) -> B`, `Item` of `Iterator` in
        // `DoubleEndedIterator<Item = A>`.
        if !data.types.iter().any(|ty| ty.name == binding.name)
            && let Some(sup) = supertrait_with_type(items, t, binding.name)
        {
            let ty = resolve_type(items, module, &binding.ty, scope, binding.ty_range, errors);
            let sup_ref = TraitRef {
                trait_: sup,
                name: items.traits[sup].name,
                args: Ty::list(&args[..items.traits[sup].generics.len().min(args.len())]),
            };
            if let Some(alias) = resolve_binding_family(
                items,
                module,
                alias_of(items, sup_ref, self_ty, binding.name),
                binding,
                scope,
                errors,
            ) {
                projections.push(Projection { alias, ty });
            }
            continue;
        }
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
        if let Some(alias) = resolve_binding_family(
            items,
            module,
            alias_of(items, trait_ref, self_ty, binding.name),
            binding,
            scope,
            errors,
        ) {
            projections.push(Projection { alias, ty });
        }
    }
    Some((trait_ref, projections))
}

/// Equality binding inputs must be represented independently from their RHS.
/// Owner type-parameter captures are recovered from the caller's generic rows;
/// explicitly written reference/lifetime arguments need occurrence tables.
fn resolve_binding_family(
    items: &ItemTree,
    module: ModuleId,
    alias: AliasTy,
    binding: &flux_hir::AssocBinding,
    scope: TypeScope<'_>,
    errors: &mut Vec<(TextRange, String)>,
) -> Option<AliasTy> {
    let alias = crate::resolve::resolve_family_arguments(
        items,
        module,
        alias,
        binding.args.as_ref(),
        scope,
        binding.range(),
        errors,
    )?;
    if alias.family_lifetimes != 0
        || (!alias.family_args.is_empty() && alias.owner_lifetimes != 0)
        || alias
            .family_args
            .iter()
            .any(|&ty| crate::regions::slots(items, ty) != 0)
    {
        errors.push((
            binding.range(),
            "lifetime-bearing associated family equality arguments are not supported yet".into(),
        ));
        return None;
    }
    Some(alias)
}

/// The supertrait of `t`, through any chain of supertraits, that has an
/// associated type `name`, if its parameters are `t`'s first ones in
/// order (or none), so that a binding of `name` in a bound on `t` can
/// name it with `t`'s arguments.
fn supertrait_with_type(items: &ItemTree, t: TraitId, name: Symbol) -> Option<TraitId> {
    let mut stack = vec![t];
    let mut seen = vec![t];
    while let Some(current) = stack.pop() {
        let data = &items.traits[current];
        for bound in data.supertraits.iter() {
            let (Some(sup), _) = items.resolve_trait(data.loc.module, bound) else {
                continue;
            };
            if seen.contains(&sup) {
                continue;
            }
            seen.push(sup);
            let last = bound.path.segments.len().saturating_sub(1);
            let forwarded = bound.path.args(last).is_none_or(|args| {
                args.bindings.is_empty()
                    && args.types.iter().enumerate().all(|(i, ty)| match ty {
                        flux_hir::TypeRef::Path(p) => {
                            p.qself.is_none()
                                && p.segments.len() == 1
                                && data.generics.params.get(i).map(|g| g.name)
                                    == Some(p.segments[0])
                        }
                        _ => false,
                    })
            });
            if !forwarded {
                continue;
            }
            if items.traits[sup].types.iter().any(|ty| ty.name == name) {
                return Some(sup);
            }
            stack.push(sup);
        }
    }
    None
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

/// A written bound must satisfy its supertrait equalities even when its
/// methods are unused. Direct contradictory where equalities alone can be
/// unused assumptions in Rust; they are checked when selecting that proof.
fn check_caller_supertraits(
    items: &ItemTree,
    sigs: &Signatures,
    scope: (&[Predicate], &[bool]),
    mut caller: Vec<Projection>,
    file: FileId,
    range: TextRange,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let (predicates, unsized_params) = scope;
    let env = elaborate(sigs, predicates);
    let required = caller_supertrait_projections(sigs, &env);
    if required.is_empty() {
        return;
    }
    caller.extend(required.iter().copied());
    // Equality of proof keys includes projections in Self and trait args,
    // but never normalizes the associated projection being constrained.
    let key = |alias: AliasTy| alias.map_args(|ty| normalize(items, sigs, &caller, ty));
    for q in required {
        let q_key = key(q.alias);
        // Moving a required equality to another receiver needs RHS evidence
        // there. Membership alone carries no associated-type constraint;
        // the equality on the original receiver must not certify that move.
        // Keep the guarded comparison when that proof context is absent.
        let receiver_proven = q_key.args[0] == q.alias.args[0]
            || caller.iter().any(|p| {
                !p.is_tautology()
                    && p.alias.args[0] == q_key.args[0]
                    && key(p.alias) == q_key
                    && normalize_projection_rhs(items, sigs, &caller, *p)
                        == normalize_projection_rhs(items, sigs, &caller, q)
            })
            || signature_impl_evidence(
                items,
                sigs,
                &env,
                unsized_params,
                &caller,
                Projection {
                    alias: q_key,
                    ty: q.ty,
                },
            );
        if caller.iter().any(|p| {
            // A written tautology cannot contradict a required equality.
            // Still check every required RHS: a recursive supertrait
            // declaration must not certify itself against a real constraint.
            !(receiver_proven && p.is_tautology() && key(p.alias) != p.alias && p.alias != q.alias)
                && key(p.alias) == q_key
                && normalize_projection_rhs(items, sigs, &caller, *p)
                    != normalize_projection_rhs(items, sigs, &caller, q)
        }) {
            diagnostics.push(Diagnostic {
                file,
                range,
                message: format!("type annotations needed: cannot satisfy `{q}`"),
            });
            break;
        }
    }
}

/// Rust's ordered orphan check: a local trait, or a local input type with
/// no uncovered type parameters before it. References and Box are fundamental
/// wrappers; other foreign constructors cover parameters but do not confer locality.
fn orphan_error(items: &ItemTree, sigs: &Signatures, id: ImplId) -> Option<String> {
    let imp = &items.impls[id];
    let trait_id = imp.trait_?;
    let own = items.modules[imp.module].krate;
    if items.is_core(imp.module) || items.modules[items.traits[trait_id].loc.module].krate == own {
        return None;
    }
    fn fundamental_inner(ty: Ty) -> Option<Ty> {
        if let TyKind::Ref(_, inner) = ty.kind() {
            Some(inner)
        } else {
            flux_solve::box_pointee(ty)
        }
    }
    fn local(items: &ItemTree, own: flux_hir::CrateId, ty: Ty) -> bool {
        if let Some(inner) = fundamental_inner(ty) {
            return local(items, own, inner);
        }
        match ty.kind() {
            TyKind::Adt(adt, _) => items.modules[items.structs[adt.strukt].loc.module].krate == own,
            TyKind::Dynamic(d) => items.modules[items.traits[d.trait_].loc.module].krate == own,
            _ => false,
        }
    }
    fn uncovered(ty: Ty) -> Option<Symbol> {
        if let Some(inner) = fundamental_inner(ty) {
            return uncovered(inner);
        }
        if let TyKind::Param(p) = ty.kind() {
            Some(p.name)
        } else {
            None
        }
    }
    let self_ty = *sigs.impls.get(id)?;
    let args = sigs.impl_traits.get(id)?.args;
    let projections = sigs.impl_projections.get(id).map_or(&[][..], |p| p);
    let inputs: Vec<_> = std::iter::once(self_ty)
        .chain(args.iter().copied())
        .map(|ty| normalize(items, sigs, projections, ty))
        .collect();
    if inputs.contains(&Ty::ERROR) {
        return None;
    }
    let Some(first) = inputs.iter().position(|&ty| local(items, own, ty)) else {
        let kind = match self_ty.kind() {
            TyKind::Adt(..) => "types defined outside of the crate",
            TyKind::Int(_) | TyKind::Bool | TyKind::Char | TyKind::Str => "primitive types",
            _ => "arbitrary types",
        };
        return Some(format!(
            "only traits defined in the current crate can be implemented for {kind}"
        ));
    };
    if let Some(param) = inputs[..first].iter().find_map(|&ty| {
        uncovered(ty).filter(|name| {
            !imp.generics
                .params
                .iter()
                .any(|p| p.name == *name && p.is_const())
        })
    }) {
        return Some(format!(
            "type parameter `{param}` must be covered by another type when it appears before the first local type"
        ));
    }
    None
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
        // A trait object implements its trait and supertraits itself.
        if let Some(&self_ty) = sigs.impls.get(id)
            && let TyKind::Dynamic(d) = self_ty.kind()
            && flux_solve::objects::supertraits_of(items, d.trait_).contains(&t)
        {
            error(
                imp.def_range,
                format!(
                    "the object type `({self_ty} + 'static)` automatically implements the trait `{}`",
                    data.name
                ),
            );
            continue;
        }
        if let Some(message) = orphan_error(items, sigs, id) {
            error(imp.def_range, message);
            continue;
        }
        // Only `core` implements the `Fn`-family traits on stable Rust.
        if crate::fn_trait_kind(items, t).is_some() && !items.is_core(imp.module) {
            let at = imp.trait_ref.as_ref().map_or(imp.def_range, |b| b.range);
            error(
                at,
                format!("manual implementations of `{}` are experimental", data.name),
            );
            continue;
        }
        if is_auto_trait(items, t) && !items.is_core(imp.module) {
            // Selection erases lifetimes outside bodies. Restrict these
            // impls to uniformly quantified lifetimes until both solvers
            // can enforce their region-dependent applicability.
            match sigs.impls.get(id).map(|ty| ty.kind()) {
                Some(TyKind::Adt(..)) => {
                    let lifetimes = crate::regions::slots(items, sigs.impls[id]);
                    let regular_lifetimes = sigs.regions.impls.get(id).is_some_and(|r| {
                        // Named and elided header lifetimes are both universal.
                        // Accept a permutation of distinct binder parameters;
                        // constants and repeated parameters specialize the impl.
                        r.names.len() == lifetimes
                            && r.self_ty.len() == lifetimes
                            && r.self_ty.iter().enumerate().all(|(i, region)| {
                                matches!(region, Region::Param(p) if (*p as usize) < r.names.len())
                                    && !r.self_ty[..i].contains(region)
                            })
                            // The signature solver erases regions. It cannot
                            // decide applicability of a region-bearing bound.
                            && r.bounds.is_empty()
                            && r.bound_selfs.len() == sigs.impl_predicates.get(id).map_or(0, |p| p.len())
                            && r.bound_traits.len() == r.bound_selfs.len()
                            && r.projections.len() == sigs.impl_projections.get(id).map_or(0, |p| p.len())
                            && r.bound_selfs.iter().all(|r| r.is_empty())
                            && r.bound_traits.iter().all(|r| {
                                r.lifetimes.is_empty() && r.args.iter().all(|a| a.is_empty())
                            })
                            && r.projections.iter().all(|r| r.is_empty())
                    });
                    if !regular_lifetimes
                        || !imp.generics.outlives.is_empty()
                        || !imp.generics.type_outlives.is_empty()
                    {
                        error(
                            imp.self_range,
                            "lifetime specialization and bounds in auto trait impls are not supported yet".into(),
                        );
                    }
                }
                _ => error(
                    imp.self_range,
                    "auto traits can only be implemented for structs and enums".into(),
                ),
            }
        }
        let has_may_dangle = imp.generics.params.iter().any(|p| p.may_dangle)
            || imp.generics.lifetimes.iter().any(|p| p.may_dangle);
        let internal_drop = data.lang == Some(flux_intern::sym::drop) && has_may_dangle;
        if has_may_dangle && (!internal_drop || !imp.is_unsafe) {
            error(
                imp.def_range,
                "`may_dangle` requires an unsafe `Drop` impl".into(),
            );
        }
        match (imp.is_unsafe, data.is_unsafe) {
            (true, false) if !internal_drop => error(
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
            let mut projections = caller_owner_projections(sigs, Some(Owner::Impl(id)));
            projections.extend(caller_supertrait_projections(sigs, &env));
            let parameters =
                flux_solve::unsized_params(items, Some(Owner::Impl(id)), &Generics::default());
            let proof = SignatureImplProof {
                items,
                sigs,
                env: env
                    .iter()
                    .map(|&p| SignatureImplProof::normalize(items, sigs, &projections, p))
                    .collect(),
                memberships: &env,
                projections,
                unsized_params: &parameters,
                depth: std::cell::Cell::new(0),
            };
            for sup in sigs.traits.get(t).map_or(&[][..], |s| &s.supertraits) {
                // `trait Try: FromResidual<<Self as Try>::Residual>`
                // names the block's own types.
                let goal = sup.subst(&args).with_self(self_ty);
                let goal = Predicate {
                    self_ty: proof.normalize_ty(goal.self_ty, None),
                    trait_ref: TraitRef {
                        args: crate::ty::map_list(goal.trait_ref.args, |t| {
                            proof.normalize_ty(t, None)
                        }),
                        ..goal.trait_ref
                    },
                };
                if !proof.holds(goal, &mut Vec::new()) {
                    error(imp.self_range, unmet(items, goal));
                }
            }
        }
    }
}

/// Detect a family definition returning to an associated name on the same
/// impl header. Ignore changing family arguments, which cannot make such a
/// definition terminate; keep projections on another Self abstract.
fn associated_family_cycle(sigs: &Signatures, imp: ImplId, name: Symbol) -> bool {
    let (Some(&self_ty), Some(&trait_ref)) = (sigs.impls.get(imp), sigs.impl_traits.get(imp))
    else {
        return false;
    };
    let mut pending = vec![name];
    let mut visited = Vec::new();
    while let Some(current) = pending.pop() {
        if visited.contains(&current) {
            continue;
        }
        visited.push(current);
        let Some(rhs) = impl_type(sigs, imp, current) else {
            continue;
        };
        let mut types = vec![rhs];
        let mut seen = rustc_hash::FxHashSet::default();
        while let Some(ty) = types.pop() {
            if !seen.insert(ty) {
                continue;
            }
            if let TyKind::Alias(alias) = ty.kind()
                && alias.trait_ == trait_ref.trait_
                && alias.args.first() == Some(&self_ty)
                && alias.args.get(1..) == Some(trait_ref.args)
            {
                if alias.name == name {
                    return true;
                }
                pending.push(alias.name);
            }
            types.extend(ty.children());
        }
    }
    false
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
    let mut args: Vec<Ty> = std::iter::once(self_ty)
        .chain(trait_ref.args.iter().copied())
        .collect();
    let declaration = items.traits[trait_ref.trait_]
        .types
        .iter()
        .find(|required| required.name == ty.name);
    if let Some(declaration) = declaration
        && (declaration.generics.lifetimes.len() != ty.generics.lifetimes.len()
            || declaration.generics.params.len() != ty.generics.params.len()
            || declaration
                .generics
                .params
                .iter()
                .zip(&ty.generics.params)
                .any(|(required, defined)| {
                    crate::param_const_ty(required) != crate::param_const_ty(defined)
                }))
    {
        error(
            ty.ty_range,
            "associated type generic parameters do not match the trait declaration".into(),
        );
        return;
    }
    if (!ty.generics.params.is_empty() || !ty.generics.lifetimes.is_empty())
        && associated_family_cycle(sigs, imp, ty.name)
    {
        error(ty.ty_range, "cycle in associated type family".into());
        return;
    }
    args.extend(crate::params_of(
        &ty.generics,
        items.impls[imp].generics.params.len(),
    ));
    let family = sig
        .families
        .iter()
        .find(|(name, _)| *name == ty.name)
        .map(|(_, family)| family);
    let mut env = sigs
        .impl_predicates
        .get(imp)
        .map_or(Vec::new(), |predicates| predicates.to_vec());
    if let Some(family) = family {
        env.extend(
            family
                .predicates
                .iter()
                .map(|predicate| predicate.subst(&args)),
        );
    }
    let env = elaborate(sigs, &env);
    let own_family = sigs
        .impl_families
        .get(imp)
        .and_then(|families| families.iter().find(|(name, _)| *name == ty.name))
        .map(|(_, family)| family);
    if let Some(family) = family {
        let mut declared_env = sigs
            .impl_predicates
            .get(imp)
            .map_or(Vec::new(), |predicates| predicates.to_vec());
        if let Some(own) = own_family {
            declared_env.extend(own.predicates.iter().copied());
        }
        let declared_env = elaborate(sigs, &declared_env);
        let mut provided = sigs
            .impl_projections
            .get(imp)
            .map_or(Vec::new(), |projections| projections.to_vec());
        if let Some(own) = own_family {
            provided.extend(own.projections.iter().copied());
        }
        let implementation_parameters =
            flux_solve::unsized_params(items, Some(Owner::Impl(imp)), &ty.generics);
        let proof = SignatureImplProof {
            items,
            sigs,
            env: declared_env.clone(),
            memberships: &declared_env,
            projections: provided,
            unsized_params: &implementation_parameters,
            depth: std::cell::Cell::new(0),
        };
        for required in &family.predicates {
            let required = normalize_predicate(items, sigs, required.subst(&args));
            if !proof.holds(required, &mut Vec::new()) {
                error(
                    ty.ty_range,
                    "associated type impl does not satisfy its declaration bounds".into(),
                );
                return;
            }
        }
        if let (Some(declaration), Some(sized)) = (declaration, sigs.builtins.sized) {
            for (index, parameter) in declaration.generics.params.iter().enumerate() {
                if crate::param_const_ty(parameter).is_none() && !parameter.maybe_unsized {
                    let goal = Predicate {
                        self_ty: args[1 + trait_ref.args.len() + index],
                        trait_ref: TraitRef {
                            trait_: sized,
                            name: items.traits[sized].name,
                            args: &[],
                        },
                    };
                    if !proof.holds(goal, &mut Vec::new()) {
                        error(
                            ty.ty_range,
                            "associated type impl does not satisfy its declaration bounds".into(),
                        );
                        return;
                    }
                }
            }
        }
        for required in &family.projections {
            let required = required.subst(&args);
            if proof.normalize_ty(Ty::alias(required.alias), None) != proof.normalize_rhs(required)
            {
                error(
                    ty.ty_range,
                    "associated type impl does not satisfy its declaration equalities".into(),
                );
                return;
            }
        }
    }
    let mut projections = sigs
        .impl_projections
        .get(imp)
        .map_or(Vec::new(), |projections| projections.to_vec());
    if let Some(family) = family {
        projections.extend(
            family
                .projections
                .iter()
                .map(|projection| projection.subst(&args)),
        );
    }
    let projections = projections.as_slice();
    let mut declaration_parameters =
        flux_solve::unsized_params(items, Some(Owner::Impl(imp)), &ty.generics);
    if let Some(declaration) = declaration {
        let owner_count = items.impls[imp].generics.params.len();
        for (index, parameter) in declaration.generics.params.iter().enumerate() {
            declaration_parameters[owner_count + index] = parameter.maybe_unsized;
        }
    }
    let declaration_proof = SignatureImplProof {
        items,
        sigs,
        env: env.clone(),
        memberships: &env,
        projections: projections.to_vec(),
        unsized_params: &declaration_parameters,
        depth: std::cell::Cell::new(0),
    };
    if let Some(sized) = sigs.builtins.sized {
        for (index, parameter) in ty.generics.params.iter().enumerate() {
            if crate::param_const_ty(parameter).is_none() && !parameter.maybe_unsized {
                let goal = Predicate {
                    self_ty: args[1 + trait_ref.args.len() + index],
                    trait_ref: TraitRef {
                        trait_: sized,
                        name: items.traits[sized].name,
                        args: &[],
                    },
                };
                if !declaration_proof.holds(goal, &mut Vec::new()) {
                    error(
                        ty.ty_range,
                        "associated type impl has stricter bounds than its trait".into(),
                    );
                    return;
                }
            }
        }
    }
    if let Some(own) = sigs
        .impl_families
        .get(imp)
        .and_then(|families| families.iter().find(|(name, _)| *name == ty.name))
        .map(|(_, family)| family)
    {
        for &predicate in &own.predicates {
            let predicate = normalize_predicate(items, sigs, predicate);
            if !declaration_proof.holds(predicate, &mut Vec::new()) {
                error(
                    ty.ty_range,
                    "associated type impl has stricter bounds than its trait".into(),
                );
                return;
            }
        }
        for &projection in &own.projections {
            if normalize(items, sigs, projections, Ty::alias(projection.alias))
                != normalize(items, sigs, projections, projection.ty)
            {
                error(
                    ty.ty_range,
                    "associated type impl has stricter bounds than its trait".into(),
                );
                return;
            }
        }
    }
    let t = normalize(items, sigs, projections, t);
    let parameters = flux_solve::unsized_params(items, Some(Owner::Impl(imp)), &ty.generics);
    if declaration.is_some_and(|declaration| !declaration.maybe_unsized)
        && flux_solve::maybe_unsized(items, sigs, t, &parameters, &env)
    {
        error(ty.ty_range, "associated type output must be sized".into());
        return;
    }

    for &(name, bound) in sig.type_bounds.iter() {
        if name != ty.name {
            continue;
        }
        let goal = bound.subst(&args).with_self(t);
        let goal = Predicate {
            self_ty: goal.self_ty,
            trait_ref: TraitRef {
                args: crate::ty::map_list(goal.trait_ref.args, |a| {
                    normalize(items, sigs, projections, a)
                }),
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
        let alias = q.alias.map_args(|a| normalize(items, sigs, projections, a));
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
    // In particular, an unsafe baseline target-feature method cannot satisfy
    // a safe trait declaration: calls through the trait must preserve safety.
    if function.is_unsafe != decl.is_unsafe || function.abi != decl.abi {
        return error(
            function.def_range,
            format!("method `{name}` has an incompatible type for trait"),
        );
    }
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
    // The block's bounds' associated types, and its own: `Self::IntoIter`
    // in `impl<I: Iterator> IntoIterator for I` is `I`.
    let mut projections = sigs
        .impl_projections
        .get(imp)
        .map_or(Vec::new(), |p| p.to_vec());
    for &(name, ty) in sigs.impl_types.get(imp).map_or(&[][..], |t| t) {
        if items.impls[imp]
            .types
            .iter()
            .find(|definition| definition.name == name)
            .is_some_and(|definition| {
                !definition.generics.params.is_empty() || !definition.generics.lifetimes.is_empty()
            })
        {
            continue;
        }
        projections.push(Projection {
            alias: alias_of(items, *trait_ref, self_ty, name),
            ty,
        });
    }
    let projections = &projections[..];
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
        .find(|((got, want), _)| {
            !flux_solve::fn_regions::same_shape(**got, **want)
                && **got != Ty::ERROR
                && **want != Ty::ERROR
        });
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
    // Both sides with the block's associated types normalized, as the
    // signatures are: the trait's `F: FnMut(B, Self::Item)` is the
    // block's `F: FnMut(B, I::Item)`.
    let norm_predicate = |p: Predicate| {
        let args =
            crate::ty::map_list(p.trait_ref.args, |a| normalize(items, sigs, projections, a));
        Predicate {
            self_ty: normalize(items, sigs, projections, p.self_ty),
            trait_ref: TraitRef {
                args,
                ..p.trait_ref
            },
        }
    };
    let declared = sigs.fn_predicates.get(required).map_or(&[][..], |p| p);
    env.extend(declared.iter().map(|p| norm_predicate(p.subst(&args))));
    let env = elaborate(sigs, &env);
    // A trait's opaque result is an output family. Its implementation may
    // refine the concrete type, but must retain every promised bound and
    // associated output equality.
    if let TyKind::Opaque(opaque) = sigs[required].ret.subst(&args).kind() {
        let replace = |ty: Ty| {
            ty.map_opaques(&mut |found| {
                if found == opaque {
                    sig.ret
                } else {
                    Ty::opaque(found)
                }
            })
        };
        for bound in flux_solve::traits::opaque_bounds(sigs, opaque) {
            let bound = Predicate {
                self_ty: replace(bound.self_ty),
                trait_ref: crate::TraitRef {
                    args: crate::ty::map_list(bound.trait_ref.args, replace),
                    ..bound.trait_ref
                },
            };
            if !holds(items, sigs, &env, norm_predicate(bound)) {
                error(
                    ret_range,
                    "return type does not satisfy trait method bounds".to_owned(),
                );
            }
        }
        for projection in flux_solve::traits::opaque_projections(sigs, opaque) {
            let actual = normalize(
                items,
                sigs,
                projections,
                Ty::alias(projection.alias.map_args(replace)),
            );
            let expected = normalize(items, sigs, projections, replace(projection.ty));
            if !flux_solve::fn_regions::same_shape(actual, expected) {
                error(
                    ret_range,
                    "return type has an incompatible associated type for trait method".to_owned(),
                );
            }
        }
    }
    for (written, &p) in function.generics.predicates.iter().zip(own) {
        if !holds(items, sigs, &env, norm_predicate(p)) {
            error(
                written.bound.range,
                "impl has stricter requirements than trait".to_owned(),
            );
        }
    }
}

/// Normalize a family selected by one local structural impl header. Generic
/// families with abstract inputs, competing headers, or upstream declarations
/// remain opaque; projection inversion never binds an intersection variable.
fn coherence_header_ty(items: &ItemTree, sigs: &Signatures, own: flux_hir::CrateId, ty: Ty) -> Ty {
    fn walk(
        items: &ItemTree,
        sigs: &Signatures,
        own: flux_hir::CrateId,
        ty: Ty,
        active: &mut Vec<Ty>,
    ) -> Ty {
        if !ty.has_aliases() || active.len() >= 64 || active.contains(&ty) {
            return ty;
        }
        active.push(ty);
        let nested = ty.map_children(|child| walk(items, sigs, own, child, active));
        let normalized = if !nested.has_params() && !nested.has_infer() && !nested.has_opaques() {
            normalize(items, sigs, &[], nested)
        } else if let TyKind::Alias(alias) = nested.kind()
            && !is_rigid(alias.args[0])
        {
            // A known constructor can choose one family declaration even with
            // rigid generic arguments. Competing headers remain possible under
            // specialization; their bounds are not negative evidence here.
            let goal = alias_predicate(alias);
            let goal_header: Vec<Ty> = std::iter::once(goal.self_ty)
                .chain(goal.trait_ref.args.iter().copied())
                .collect();
            let mut candidates = sigs
                .impl_index
                .impls_for(items, goal.trait_ref.trait_, goal.self_ty)
                .filter(|&imp| {
                    let header: Vec<Ty> = std::iter::once(sigs.impls[imp])
                        .chain(sigs.impl_traits[imp].args.iter().copied())
                        .collect();
                    unify_headers(&header, &goal_header).is_some()
                });
            let unique = candidates.next().filter(|_| candidates.next().is_none());
            if let Some((imp, args)) = select_impl(items, sigs, goal)
                && Some(imp) == unique
                && items.modules[items.impls[imp].module].krate == own
                && let Some(rhs) = instantiate_impl_type(sigs, imp, alias, &args)
            {
                walk(items, sigs, own, rhs, active)
            } else {
                nested
            }
        } else {
            nested
        };
        active.pop();
        normalized
    }
    walk(items, sigs, own, ty, &mut Vec::new())
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
    let Some(args) = match_impl_sized(items, sigs, general, goal, false) else {
        return false;
    };
    // A parameter not bounded `?Sized` is not an unsized type, as
    // rustc's implicit `Sized` bounds say: `impl<T: Clone> ToOwned for
    // T` is not for `str`.
    if match_impl(items, sigs, general, goal).is_none() {
        return true;
    }
    let env = elaborate(sigs, sigs.impl_predicates.get(specific).map_or(&[], |p| p));
    let bounds = sigs.impl_predicates.get(general).map_or(&[][..], |p| p);
    bounds
        .iter()
        .any(|p| !holds(items, sigs, &env, p.subst(&args)))
}

/// Intersected headers retain implicit Sized requirements. A local bound also
/// cannot hold for an orphan-closed constructor without an existing proof:
/// downstream crates cannot add that impl (`&[T]`, for example).
fn intersection_excluded_by_bounds(
    items: &ItemTree,
    sigs: &Signatures,
    a: ImplId,
    b: ImplId,
) -> bool {
    fn resolved(ty: Ty) -> bool {
        !ty.has_params() && !ty.has_infer() && !ty.has_aliases() && !ty.has_opaques()
    }
    fn closed(items: &ItemTree, ty: Ty) -> bool {
        match ty.kind() {
            TyKind::Ref(_, inner) => closed(items, inner),
            TyKind::Adt(adt, args)
                if flux_solve::is_lang_struct(items, adt.strukt, "owned_box") =>
            {
                args.first().is_some_and(|&ty| closed(items, ty))
            }
            TyKind::Param(_)
            | TyKind::Alias(_)
            | TyKind::Infer(_)
            | TyKind::Error
            | TyKind::Opaque(_) => false,
            // The fixed principal trait determines locality, independently of
            // its generic arguments. Implicit object proofs are checked below.
            TyKind::Dynamic(_) => true,
            _ => true,
        }
    }
    let own = items.modules[items.impls[b].module].krate;
    let bounds_of = |id: ImplId| {
        let mut bounds = sigs
            .impl_predicates
            .get(id)
            .map_or(&[][..], |bounds| &**bounds)
            .to_vec();
        // Header matching checks implicit Sized separately. The symmetric
        // intersection must carry the same requirement through substitution.
        if let Some(sized) = sigs.builtins.sized {
            for (index, param) in items.impls[id].generics.params.iter().enumerate() {
                if !param.is_const() && !param.maybe_unsized {
                    bounds.push(Predicate {
                        self_ty: Ty::param(index as u32, param.name),
                        trait_ref: TraitRef {
                            trait_: sized,
                            name: items.traits[sized].name,
                            args: &[],
                        },
                    });
                }
            }
        }
        bounds
    };
    let a_bounds = bounds_of(a);
    let b_bounds = bounds_of(b);
    let local = |predicate: &Predicate| {
        items.modules[items.traits[predicate.trait_ref.trait_].loc.module].krate == own
    };
    let header = |id| {
        std::iter::once(sigs.impls[id])
            .chain(sigs.impl_traits[id].args.iter().copied())
            .map(|ty| coherence_header_ty(items, sigs, own, ty))
            .collect::<Vec<_>>()
    };
    let Some((_, bounds)) =
        unify_headers_with_predicates(&header(a), &header(b), &a_bounds, &b_bounds)
    else {
        return false;
    };
    bounds.iter().any(|predicate| {
        if sigs.builtins.sized == Some(predicate.trait_ref.trait_)
            && sigs.is_unsized(predicate.self_ty)
        {
            return true;
        }
        let open_object_bound = match predicate.self_ty.kind() {
            TyKind::Dynamic(object) => {
                flux_solve::objects::supertraits_of(items, object.trait_)
                    .contains(&predicate.trait_ref.trait_)
                    && (!resolved(predicate.self_ty)
                        || !predicate.trait_ref.args.iter().copied().all(resolved))
            }
            _ => false,
        };
        let absent_local = local(predicate)
            && closed(items, predicate.self_ty)
            && predicate.trait_ref.args.iter().copied().all(|ty| closed(items, ty))
            && !open_object_bound
            // Objects and builtin traits can satisfy bounds without an impl
            // block. Absence from the index alone is not a negative proof.
            && !holds(items, sigs, &[], *predicate)
            && sigs
                .impl_index
                .impls_for(items, predicate.trait_ref.trait_, predicate.self_ty)
                .next()
                .is_none();
        absent_local || coherence_goal_impossible(items, sigs, own, *predicate, &mut Vec::new())
    })
}

/// A negative proof for an orphan-knowable intersection goal. Generic inputs
/// permit only a proof that no candidate header can match. Concrete inputs can
/// follow impossible prerequisites through blanket adapters. Missing upstream
/// impls, unresolved projections and recursive bounds remain unknown.
fn coherence_goal_impossible(
    items: &ItemTree,
    sigs: &Signatures,
    own: flux_hir::CrateId,
    goal: Predicate,
    active: &mut Vec<Predicate>,
) -> bool {
    fn represented(ty: Ty) -> bool {
        !has_error(ty) && !ty.has_aliases() && !ty.has_opaques()
    }
    fn closed_input(items: &ItemTree, ty: Ty) -> bool {
        match ty.kind() {
            TyKind::Ref(_, inner) => closed_input(items, inner),
            TyKind::Adt(adt, _) if flux_solve::is_lang_struct(items, adt.strukt, "owned_box") => {
                flux_solve::box_pointee(ty).is_some_and(|inner| closed_input(items, inner))
            }
            TyKind::Param(_)
            | TyKind::Alias(_)
            | TyKind::Infer(_)
            | TyKind::Error
            | TyKind::Opaque(_) => false,
            _ => true,
        }
    }
    fn local_input(items: &ItemTree, own: flux_hir::CrateId, ty: Ty) -> bool {
        match ty.kind() {
            TyKind::Ref(_, inner) => local_input(items, own, inner),
            TyKind::Adt(adt, _)
                if items.modules[items.structs[adt.strukt].loc.module].krate == own =>
            {
                true
            }
            TyKind::Adt(adt, _) if flux_solve::is_lang_struct(items, adt.strukt, "owned_box") => {
                flux_solve::box_pointee(ty).is_some_and(|inner| local_input(items, own, inner))
            }
            TyKind::Dynamic(d) => items.modules[items.traits[d.trait_].loc.module].krate == own,
            _ => false,
        }
    }
    if active.len() >= 32 {
        return false;
    }
    let goal = normalize_predicate(items, sigs, goal);
    if !represented(goal.self_ty)
        || !goal.trait_ref.args.iter().copied().all(represented)
        || active.contains(&goal)
        || holds(items, sigs, &[], goal)
    {
        return false;
    }
    let local_trait = items.modules[items.traits[goal.trait_ref.trait_].loc.module].krate == own;
    let has_local_input = std::iter::once(goal.self_ty)
        .chain(goal.trait_ref.args.iter().copied())
        .any(|ty| local_input(items, own, ty));
    if !local_trait && !has_local_input {
        return false;
    }
    if std::iter::once(goal.self_ty)
        .chain(goal.trait_ref.args.iter().copied())
        .any(|ty| ty.has_params() || ty.has_infer())
    {
        // A nominal constructor containing rigid parameters cannot become a
        // downstream-owned type. Fundamental wrappers and bare parameters can.
        // Only prove absence: any possibly matching candidate stays unknown,
        // regardless of bounds that might fail for a particular substitution.
        if !std::iter::once(goal.self_ty)
            .chain(goal.trait_ref.args.iter().copied())
            .all(|ty| closed_input(items, ty))
            || items.traits[goal.trait_ref.trait_].lang.is_some()
            || matches!(goal.self_ty.kind(), TyKind::Dynamic(object)
                if flux_solve::objects::supertraits_of(items, object.trait_)
                    .contains(&goal.trait_ref.trait_))
        {
            return false;
        }
        let goal_header: Vec<Ty> = std::iter::once(goal.self_ty)
            .chain(goal.trait_ref.args.iter().copied())
            .collect();
        return sigs
            .impl_index
            .impls_for(items, goal.trait_ref.trait_, goal.self_ty)
            .all(|imp| {
                let (Some(&self_ty), Some(trait_ref)) =
                    (sigs.impls.get(imp), sigs.impl_traits.get(imp))
                else {
                    return false;
                };
                let header: Vec<Ty> = std::iter::once(self_ty)
                    .chain(trait_ref.args.iter().copied())
                    .map(|ty| coherence_header_ty(items, sigs, own, ty))
                    .collect();
                unify_headers(&header, &goal_header).is_none()
            });
    }
    // Builtin structural candidates need their own prerequisites too. A failed
    // positive proof by itself never means that there is no builtin candidate.
    if let Some(requirements) = auto_trait_requirements(items, sigs, goal) {
        active.push(goal);
        let impossible = requirements
            .into_iter()
            .any(|p| coherence_goal_impossible(items, sigs, own, p, active));
        active.pop();
        if !impossible {
            return false;
        }
    }
    active.push(goal);
    let impossible = sigs
        .impl_index
        .impls_for(items, goal.trait_ref.trait_, goal.self_ty)
        .all(|imp| {
            // An indexed impl can lack a resolved trait header after an error.
            // It cannot establish that the goal is impossible.
            let (Some(&self_ty), Some(trait_ref)) =
                (sigs.impls.get(imp), sigs.impl_traits.get(imp))
            else {
                return false;
            };
            let Some(args) = match_impl(items, sigs, imp, goal) else {
                // An unresolved projected header might match after its own
                // constraints are solved. That is not a known disjoint candidate.
                return !self_ty.has_aliases() && !trait_ref.args.iter().any(|ty| ty.has_aliases());
            };
            sigs.impl_predicates
                .get(imp)
                .into_iter()
                .flatten()
                .any(|p| coherence_goal_impossible(items, sigs, own, p.subst(&args), active))
        });
    active.pop();
    impossible
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

/// Substituted declaration bounds on a trait's non-Self type parameters.
fn trait_argument_requirements<'a>(
    items: &'a ItemTree,
    sigs: &'a Signatures,
    predicate: Predicate,
) -> impl Iterator<Item = Predicate> + 'a {
    let args: Vec<_> = std::iter::once(predicate.self_ty)
        .chain(predicate.trait_ref.args.iter().copied())
        .collect();
    let declared = sigs
        .trait_predicates
        .get(predicate.trait_ref.trait_)
        .map_or(&[][..], |p| p.as_ref());
    let explicit = declared.iter().map(move |p| p.subst(&args));
    let implicit = items.traits[predicate.trait_ref.trait_]
        .generics
        .params
        .iter()
        .zip(predicate.trait_ref.args)
        .filter(|(p, _)| !p.is_const() && !p.maybe_unsized)
        .filter_map(move |(_, &ty)| {
            sigs.builtins.sized.map(|id| {
                TraitRef {
                    trait_: id,
                    name: items.traits[id].name,
                    args: &[],
                }
                .with_self(ty)
            })
        });
    explicit.chain(implicit)
}

/// Owner equalities exclude contracts quantified by a family's own binders.
fn trait_owner_projections(sigs: &Signatures, id: TraitId) -> Vec<Projection> {
    let declared = sigs.trait_projections.get(id).map_or(&[][..], |p| p);
    let Some(signature) = sigs.traits.get(id) else {
        return declared.to_vec();
    };
    declared
        .iter()
        .enumerate()
        .filter_map(|(index, &projection)| {
            if index < signature.header_projections.len() {
                return Some(projection);
            }
            let TyKind::Alias(family) = projection.alias.self_ty().kind() else {
                return Some(projection);
            };
            if family.trait_ != id {
                return Some(projection);
            }
            let local = signature
                .families
                .iter()
                .find(|(name, _)| *name == family.name)
                .map(|(_, family)| family);
            local
                .is_none_or(|family| {
                    family.lifetimes == 0
                        && family.params.is_empty()
                        && family.predicates.is_empty()
                        && family.projections.is_empty()
                })
                .then_some(projection)
        })
        .collect()
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
    // Nominal and trait argument bounds both create declaration obligations.
    // Without either, only associated types require well-formedness checks.
    let struct_bounds = sigs.struct_predicates.iter().next().is_some()
        || sigs.struct_projections.iter().next().is_some();
    let declaration_bounds =
        struct_bounds || sigs.trait_predicates.iter().any(|(_, p)| !p.is_empty());
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
        check_caller_supertraits(
            items,
            sigs,
            (own, &flux_solve::unsized_params(items, None, &s.generics)),
            declared.to_vec(),
            s.loc.file,
            s.def_range,
            diagnostics,
        );
        if !declaration_bounds && !aliased(sigs.structs[id]) && !aliased_bounds(own, declared) {
            continue;
        }
        let (env, projections) = scope(own.to_vec(), declared.to_vec());
        let wf = WfScope {
            unsized_params: flux_solve::unsized_params(items, None, &s.generics),
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
        check_caller_supertraits(
            items,
            sigs,
            (
                own,
                &flux_solve::unsized_params(items, Some(Owner::Impl(id)), &Generics::default()),
            ),
            declared.to_vec(),
            imp.file,
            imp.def_range,
            diagnostics,
        );
        let types = sigs.impl_types.get(id).map_or(&[][..], |t| t);
        let trait_args = sigs.impl_traits.get(id).map_or(&[][..], |t| t.args);
        if !declaration_bounds
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
            unsized_params: flux_solve::unsized_params(
                items,
                Some(Owner::Impl(id)),
                &Generics::default(),
            ),
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
            check.trait_arguments(t.with_self(sigs.impls[id]), bound.range);
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
        check.bounds(&imp.generics, own, declared);
        for (written, &(_, ty)) in imp.types.iter().zip(types) {
            let family = sigs
                .impl_families
                .get(id)
                .and_then(|families| families.iter().find(|(name, _)| *name == written.name))
                .map(|(_, family)| family);
            let mut predicates = own.to_vec();
            let mut equalities = declared.to_vec();
            if let Some(family) = family {
                predicates.extend(family.predicates.iter().copied());
                equalities.extend(family.projections.iter().copied());
            }
            let (env, projections) = scope(predicates, equalities);
            let wf = WfScope {
                unsized_params: flux_solve::unsized_params(
                    items,
                    Some(Owner::Impl(id)),
                    &written.generics,
                ),
                env: &env,
                projections: &projections,
                aliases: true,
            };
            let mut check =
                WfCheck::new(items, sigs, imp.file, &wf, written.item_range, diagnostics);
            check.ty(ty, Some(&written.ty), WfRange::at(written.ty_range));
            if let Some(family) = family {
                check.bounds(&written.generics, &family.predicates, &family.projections);
            }
        }
    }
    for (id, t) in items.traits.iter().skip(from.traits as usize) {
        let own = sigs.trait_predicates.get(id).map_or(&[][..], |p| p);
        let all_declared = sigs.trait_projections.get(id).map_or(&[][..], |p| p);
        let declared = trait_owner_projections(sigs, id);
        let header = sigs
            .traits
            .get(id)
            .map_or(&[][..], |sig| &sig.header_projections);
        check_caller_supertraits(
            items,
            sigs,
            (
                &owner_predicates(items, sigs, Some(Owner::Trait(id))),
                &flux_solve::unsized_params(items, Some(Owner::Trait(id)), &Generics::default()),
            ),
            header.to_vec(),
            t.loc.file,
            t.def_range,
            diagnostics,
        );
        if !declaration_bounds && !aliased_bounds(own, &declared) && t.types.is_empty() {
            continue;
        }
        let (env, projections) = scope(
            owner_predicates(items, sigs, Some(Owner::Trait(id))),
            Vec::new(),
        );
        let wf = WfScope {
            unsized_params: flux_solve::unsized_params(
                items,
                Some(Owner::Trait(id)),
                &Generics::default(),
            ),
            env: &env,
            projections: &projections,
            aliases: true,
        };
        let span = TextRange::new(
            t.def_range.start(),
            t.def_range.end().max(t.generics.range.end()),
        );
        let mut check = WfCheck::new(items, sigs, t.loc.file, &wf, span, diagnostics);
        check.bounds(&t.generics, own, &declared);
        if sigs.traits[id].defaults.iter().any(Option::is_some) {
            let mut predicate = trait_self_predicate(items, id);
            let mut args: Vec<_> = std::iter::once(predicate.self_ty)
                .chain(predicate.trait_ref.args.iter().copied())
                .collect();
            for (index, default) in sigs.traits[id].defaults.iter().enumerate() {
                if let Some(default) = default {
                    args[index + 1] = default.subst(&args);
                }
            }
            predicate.trait_ref.args = Ty::list(&args[1..]);
            check.trait_argument_defaults(predicate, span);
        }
        for bound in &sigs.traits[id].supertraits {
            check.trait_arguments(bound.with_self(Ty::param(0, kw::SelfUpper)), span);
        }
        for written in &t.types {
            let family = sigs
                .traits
                .get(id)
                .and_then(|signature| {
                    signature
                        .families
                        .iter()
                        .find(|(name, _)| *name == written.name)
                })
                .map(|(_, family)| family);
            let Some(family) = family else { continue };
            let mut predicates = owner_predicates(items, sigs, Some(Owner::Trait(id)));
            predicates.extend(family.predicates.iter().copied());
            let (env, mut projections) = scope(predicates, declared.clone());
            projections.extend(family.projections.iter().copied());
            let wf = WfScope {
                unsized_params: flux_solve::unsized_params(
                    items,
                    Some(Owner::Trait(id)),
                    &written.generics,
                ),
                env: &env,
                projections: &projections,
                aliases: true,
            };
            let mut check = WfCheck::new(
                items,
                sigs,
                t.loc.file,
                &wf,
                written.item_range,
                diagnostics,
            );
            check.bounds(&written.generics, &family.predicates, &family.projections);
            let mut alias = alias_of(
                items,
                trait_self_predicate(items, id).trait_ref,
                Ty::param(0, kw::SelfUpper),
                written.name,
            );
            alias.family_args = Ty::list(&crate::params_of(
                &written.generics,
                family.owner_params as usize,
            ));
            for (_, bound) in sigs.traits[id]
                .type_bounds
                .iter()
                .filter(|(name, _)| *name == written.name)
            {
                check.trait_arguments(bound.with_self(Ty::alias(alias)), written.item_range);
            }
            let output: Vec<_> = all_declared
                .iter()
                .filter(|projection| {
                    matches!(projection.alias.self_ty().kind(), TyKind::Alias(alias)
                    if alias.trait_ == id && alias.name == written.name)
                })
                .copied()
                .collect();
            check.bound_aliases(&[], &output);
        }
    }
    for (id, konst) in items.consts.iter().skip(from.consts as usize) {
        if !declaration_bounds && !sigs.consts[id].has_aliases() && !aliased_owner(konst.owner) {
            continue;
        }
        let (env, projections) = scope(owner_predicates(items, sigs, konst.owner), Vec::new());
        let wf = WfScope {
            unsized_params: flux_solve::unsized_params(items, konst.owner, &Generics::default()),
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
        check_caller_supertraits(
            items,
            sigs,
            (
                &predicates_of(items, sigs, id),
                &flux_solve::unsized_params(items, f.owner, &f.generics),
            ),
            caller_projections_of(items, sigs, id),
            f.loc.file,
            f.sig_range,
            diagnostics,
        );
        if !declaration_bounds
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
            unsized_params: flux_solve::unsized_params(items, f.owner, &f.generics),
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
            unsized_params: flux_solve::unsized_params(items, f.owner, &f.generics),
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
        if !ty.has_aliases() {
            return;
        }
        let mut work = vec![ty];
        let mut seen = rustc_hash::FxHashSet::default();
        while let Some(ty) = work.pop() {
            if !seen.insert(ty) {
                continue;
            }
            if let TyKind::Alias(alias) = ty.kind() {
                self.trait_arguments(alias_predicate(alias), range.bound);
            }
            work.extend(ty.children());
        }
    }

    /// A bound on a trait's non-Self parameter is an obligation at each
    /// occurrence, not a fact supplied by the bound being checked.
    fn trait_arguments(&mut self, predicate: Predicate, range: TextRange) {
        self.check_trait_arguments(predicate, range, false);
    }

    fn trait_argument_defaults(&mut self, predicate: Predicate, range: TextRange) {
        self.check_trait_arguments(predicate, range, true);
    }

    fn check_trait_arguments(&mut self, predicate: Predicate, range: TextRange, defaults: bool) {
        if self
            .sigs
            .trait_predicates
            .get(predicate.trait_ref.trait_)
            .is_none_or(|p| p.is_empty())
            && self.items.traits[predicate.trait_ref.trait_]
                .generics
                .params
                .iter()
                .all(|p| p.is_const() || p.maybe_unsized)
        {
            return;
        }
        let proof = SignatureImplProof {
            items: self.items,
            sigs: self.sigs,
            env: self
                .scope
                .env
                .iter()
                .map(|&p| {
                    SignatureImplProof::normalize(self.items, self.sigs, self.scope.projections, p)
                })
                .collect(),
            memberships: self.scope.env,
            projections: self.scope.projections.to_vec(),
            unsized_params: &self.scope.unsized_params,
            depth: std::cell::Cell::new(0),
        };
        let normalized = |predicate: Predicate| Predicate {
            self_ty: proof.normalize_ty(predicate.self_ty, None),
            trait_ref: TraitRef {
                args: crate::ty::map_list(predicate.trait_ref.args, |ty| {
                    proof.normalize_ty(ty, None)
                }),
                ..predicate.trait_ref
            },
        };
        let mut work = vec![(predicate, 0)];
        let mut seen = rustc_hash::FxHashSet::default();
        while let Some((predicate, depth)) = work.pop() {
            if !seen.insert(predicate) {
                continue;
            }
            if depth > 128 {
                self.report(
                    range,
                    "trait argument requirements exceed recursion limit".into(),
                );
                break;
            }
            for requirement in trait_argument_requirements(self.items, self.sigs, predicate) {
                let goal = normalized(requirement);
                // Generic defaults are conditional: Rust checks their bounds
                // when a use selects the default. Concrete invalid defaults
                // still fail at the declaration.
                if defaults
                    && (goal.self_ty.has_params()
                        || goal.trait_ref.args.iter().any(|t| t.has_params()))
                {
                    continue;
                }
                if !proof.holds(requirement, &mut Vec::new()) && !proof.holds(goal, &mut Vec::new())
                {
                    let leaf = goal;
                    if !self.reported.contains(&leaf) {
                        self.reported.push(leaf);
                        self.report(range, unmet(self.items, leaf));
                    }
                }
                work.push((goal, depth + 1));
            }
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
            self.trait_arguments(*p, written.bound.range);
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
        if let Some(Owner::Trait(trait_)) = owner {
            let declared = trait_owner_projections(self.sigs, trait_);
            let predicates = self
                .sigs
                .trait_predicates
                .get(trait_)
                .map_or(&[][..], |p| p);
            self.bound_aliases(predicates, &declared);
            return;
        }
        let (predicates, declared) = match owner {
            Some(Owner::Impl(imp)) => (
                self.sigs.impl_predicates.get(imp),
                self.sigs.impl_projections.get(imp),
            ),
            _ => return,
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
        for projection in declared.iter().filter(|p| !p.alias.family_args.is_empty()) {
            let scope = WfScope {
                unsized_params: self.scope.unsized_params.clone(),
                env: self.scope.env,
                projections: self.scope.projections,
                aliases: true,
            };
            let mut found = Vec::new();
            unmet_struct_bounds(
                self.items,
                self.sigs,
                &scope,
                Ty::alias(projection.alias),
                None,
                WfRange::at(self.span),
                &mut self.reported,
                &mut found,
            );
            for (range, message) in found {
                self.report(range, message);
            }
        }
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
            for &a in q.alias.args.iter().chain(q.alias.family_args) {
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
        let mut impls = sigs
            .impl_index
            .impls_for(items, goal.trait_ref.trait_, goal.self_ty);
        !impls.any(|other| {
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
            for &a in alias.args.iter().chain(alias.family_args) {
                collect_aliases(a, out);
            }
            out.push(alias);
        }
        TyKind::Adt(_, list) | TyKind::Tuple(list) | TyKind::Closure(_, list) => {
            list.iter().for_each(|&t| collect_aliases(t, out));
        }
        TyKind::Ref(_, t) | TyKind::RawPtr(_, t) | TyKind::Array(t, _) | TyKind::Slice(t) => {
            collect_aliases(t, out)
        }
        TyKind::FnDef(_) | TyKind::FnPtr(_) => {
            ty.children().for_each(|t| collect_aliases(t, out));
        }
        _ => {}
    }
}

/// What a well-formedness check may assume: the elaborated bounds in
/// scope, and what they say associated types are.
pub struct WfScope<'a> {
    /// Which declaration parameters may be unsized, in generic argument order.
    pub unsized_params: Vec<bool>,
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
        Some(
            TypeRef::Ref { pointee, .. }
            | TypeRef::Ptr { pointee, .. }
            | TypeRef::Array { elem: pointee, .. }
            | TypeRef::Slice(pointee),
        ) => vec![&**pointee],
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
                        args: crate::ty::map_list(p.trait_ref.args, &normalized),
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
                let alias = q.alias.map_args(&normalized);
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
            let goal = alias_predicate(alias.map_args(normalized));
            let goal = normalize_predicate(items, sigs, goal);
            let requirements = family_requirements(items, sigs, alias);
            for requirement in requirements.predicates {
                let requirement = normalize_predicate(items, sigs, requirement);
                if !holds(items, sigs, scope.env, requirement) {
                    out.push((range.bound, unmet(items, requirement)));
                }
            }
            for projection in requirements.projections {
                if normalized(Ty::alias(projection.alias)) != normalized(projection.ty) {
                    out.push((
                        range.bound,
                        "associated type family requirements are not satisfied".into(),
                    ));
                }
            }
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
        TyKind::Ref(_, t) | TyKind::RawPtr(_, t) | TyKind::Array(t, _) | TyKind::Slice(t) => {
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
    let impls = sigs
        .impl_index
        .impls_for(items, goal.trait_ref.trait_, goal.self_ty);
    let mut matching = impls.filter_map(|imp| Some((imp, match_impl(items, sigs, imp, goal)?)));
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

/// Unifies two `impl` headers, their types and trait arguments in turn,
/// each block's parameters standing for any type, consistently (so
/// `impl<T> From<T> for T` and `impl From<A> for B` do not overlap, nor
/// do `impl<T> From<T> for T` and `impl<X> From<X> for W<X>`, as
/// `X = W<X>` has no solution). Gives `b`'s header with what they share,
/// its unconstrained parameters as `_`, if they may be for the same
/// types.
fn unify_headers(a: &[Ty], b: &[Ty]) -> Option<Vec<Ty>> {
    unify_headers_with_predicates(a, b, &[], &[]).map(|(header, _)| header)
}

fn unify_headers_with_predicates(
    a: &[Ty],
    b: &[Ty],
    a_bounds: &[Predicate],
    b_bounds: &[Predicate],
) -> Option<(Vec<Ty>, Vec<Predicate>)> {
    /// A type in one block's header, and which block.
    type Side = (bool, Ty);
    type Bound = rustc_hash::FxHashMap<(bool, u32), Side>;
    fn resolve(bound: &Bound, (mut side, mut ty): Side) -> Side {
        while let TyKind::Param(p) = ty.kind()
            && let Some(&next) = bound.get(&(side, p.index))
        {
            (side, ty) = next;
        }
        (side, ty)
    }
    fn occurs(bound: &Bound, var: (bool, u32), ty: Side, depth: u32) -> bool {
        let (side, ty) = resolve(bound, ty);
        if depth > 64 {
            return false;
        }
        match ty.kind() {
            TyKind::Param(p) => (side, p.index) == var,
            TyKind::Adt(_, list) | TyKind::Tuple(list) => list
                .iter()
                .any(|&t| occurs(bound, var, (side, t), depth + 1)),
            TyKind::Ref(_, t) | TyKind::RawPtr(_, t) | TyKind::Slice(t) => {
                occurs(bound, var, (side, t), depth + 1)
            }
            TyKind::Array(t, len) => {
                occurs(bound, var, (side, t), depth + 1)
                    || occurs(bound, var, (side, len), depth + 1)
            }
            TyKind::Dynamic(object) => object
                .args
                .iter()
                .chain(object.assoc_tys)
                .any(|&t| occurs(bound, var, (side, t), depth + 1)),
            TyKind::FnPtr(sig) => sig
                .inputs_output
                .iter()
                .any(|&t| occurs(bound, var, (side, t), depth + 1)),
            _ => false,
        }
    }
    fn unify(bound: &mut Bound, a: Side, b: Side, depth: u32) -> bool {
        if depth > 64 {
            return true;
        }
        let (a, b) = (resolve(bound, a), resolve(bound, b));
        let mut all = |xs: &[Ty], ys: &[Ty]| {
            xs.len() == ys.len()
                && xs
                    .iter()
                    .zip(ys)
                    .all(|(&s, &t)| unify(bound, (a.0, s), (b.0, t), depth + 1))
        };
        match (a.1.kind(), b.1.kind()) {
            (TyKind::Error, _) | (_, TyKind::Error) => true,
            // Residual variables in an intersected header are unconstrained.
            (TyKind::Infer(_), _) | (_, TyKind::Infer(_)) => true,
            // A projection can evaluate to either side. Do not bind an
            // unrelated generic variable by assuming its output is injective.
            (TyKind::Alias(_), _) | (_, TyKind::Alias(_)) => true,
            (TyKind::Param(p), TyKind::Param(q)) if a.0 == b.0 && p.index == q.index => true,
            (TyKind::Param(p), _) => {
                let var = (a.0, p.index);
                !occurs(bound, var, b, 0) && bound.insert(var, b).is_none()
            }
            (_, TyKind::Param(q)) => {
                let var = (b.0, q.index);
                !occurs(bound, var, a, 0) && bound.insert(var, a).is_none()
            }
            (TyKind::Adt(x, xs), TyKind::Adt(y, ys)) => x == y && all(xs, ys),
            (TyKind::Tuple(xs), TyKind::Tuple(ys)) => all(xs, ys),
            (TyKind::Ref(m, x), TyKind::Ref(n, y))
            | (TyKind::RawPtr(m, x), TyKind::RawPtr(n, y)) => {
                m == n && unify(bound, (a.0, x), (b.0, y), depth + 1)
            }
            (TyKind::Array(x, m), TyKind::Array(y, n)) => {
                unify(bound, (a.0, m), (b.0, n), depth + 1)
                    && unify(bound, (a.0, x), (b.0, y), depth + 1)
            }
            (TyKind::Slice(x), TyKind::Slice(y)) => unify(bound, (a.0, x), (b.0, y), depth + 1),
            (TyKind::Dynamic(x), TyKind::Dynamic(y)) => {
                x.trait_ == y.trait_
                    && x.auto_traits == y.auto_traits
                    && x.assoc_names == y.assoc_names
                    && all(x.args, y.args)
                    && all(x.assoc_tys, y.assoc_tys)
            }
            (TyKind::FnPtr(x), TyKind::FnPtr(y)) => {
                x.is_unsafe == y.is_unsafe
                    && x.abi == y.abi
                    && all(x.inputs_output, y.inputs_output)
            }
            _ => a.1 == b.1,
        }
    }
    fn apply(bound: &Bound, ty: Side, depth: u32) -> Ty {
        let (side, ty) = resolve(bound, ty);
        let apply_all = |list: &[Ty]| -> Vec<Ty> {
            list.iter()
                .map(|&t| apply(bound, (side, t), depth + 1))
                .collect()
        };
        match ty.kind() {
            _ if depth > 64 => ty,
            TyKind::Param(_) => Ty::infer(crate::ty::InferVar(0)),
            TyKind::Adt(adt, list) => Ty::adt(adt.strukt, adt.name, &apply_all(list)),
            TyKind::Tuple(list) => Ty::tuple(&apply_all(list)),
            TyKind::Ref(m, t) => Ty::new(TyKind::Ref(m, apply(bound, (side, t), depth + 1))),
            TyKind::RawPtr(m, t) => Ty::raw_ptr(m, apply(bound, (side, t), depth + 1)),
            TyKind::Array(t, len) => Ty::array_of(
                apply(bound, (side, t), depth + 1),
                apply(bound, (side, len), depth + 1),
            ),
            TyKind::Slice(t) => Ty::slice(apply(bound, (side, t), depth + 1)),
            TyKind::Alias(alias) => {
                Ty::alias(alias.map_args(|t| apply(bound, (side, t), depth + 1)))
            }
            TyKind::Dynamic(object) => Ty::new(TyKind::Dynamic(crate::DynTy {
                args: Ty::list(&apply_all(object.args)),
                assoc_tys: Ty::list(&apply_all(object.assoc_tys)),
                ..object
            })),
            _ => ty,
        }
    }
    let mut bound = Bound::default();
    let unified = a
        .iter()
        .zip(b)
        .all(|(&x, &y)| unify(&mut bound, (false, x), (true, y), 0));
    unified.then(|| {
        let header = b.iter().map(|&t| apply(&bound, (true, t), 0)).collect();
        let bounds = [(false, a_bounds), (true, b_bounds)]
            .into_iter()
            .flat_map(|(side, predicates)| {
                predicates
                    .iter()
                    .map(|predicate| Predicate {
                        self_ty: apply(&bound, (side, predicate.self_ty), 0),
                        trait_ref: TraitRef {
                            args: Ty::list(
                                &predicate
                                    .trait_ref
                                    .args
                                    .iter()
                                    .map(|&ty| apply(&bound, (side, ty), 0))
                                    .collect::<Vec<_>>(),
                            ),
                            ..predicate.trait_ref
                        },
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        (header, bounds)
    })
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
    for (&t, impls) in traits {
        for (i, &later) in impls.iter().enumerate() {
            if later.index() < checked as usize {
                continue;
            }
            let (Some(&b), Some(b_ref)) = (sigs.impls.get(later), sigs.impl_traits.get(later))
            else {
                continue;
            };
            let own = items.modules[items.impls[later].module].krate;
            // A type that did not resolve, or names an associated type
            // whose trait does not hold, was reported already.
            if b == Ty::ERROR || broken_header(items, sigs, later) {
                continue;
            }
            let b_header: Vec<Ty> = std::iter::once(b)
                .chain(b_ref.args.iter().copied())
                .map(|ty| coherence_header_ty(items, sigs, own, ty))
                .collect();
            let earlier = sigs.impl_index.earlier(items, sigs, t, i).find(|&a| {
                let (Some(&a_ty), Some(a_ref)) = (sigs.impls.get(a), sigs.impl_traits.get(a))
                else {
                    return false;
                };
                let a_header: Vec<Ty> = std::iter::once(a_ty)
                    .chain(a_ref.args.iter().copied())
                    .map(|ty| coherence_header_ty(items, sigs, own, ty))
                    .collect();
                a_ty != Ty::ERROR
                    && !broken_header(items, sigs, a)
                    && unify_headers(&a_header, &b_header).is_some()
                    && !excluded_by_bounds(items, sigs, a, later)
                    && !excluded_by_bounds(items, sigs, later, a)
                    && !intersection_excluded_by_bounds(items, sigs, a, later)
            });
            let Some(earlier) = earlier else {
                continue;
            };
            // What both are for, as rustc names it: `From<_>` for `W<_>`.
            let a_header: Vec<Ty> = std::iter::once(sigs.impls[earlier])
                .chain(sigs.impl_traits[earlier].args.iter().copied())
                .map(|ty| coherence_header_ty(items, sigs, own, ty))
                .collect();
            let header = unify_headers(&a_header, &b_header).unwrap_or(b_header);
            let trait_ref = TraitRef {
                args: Ty::list(&header[1..]),
                ..*b_ref
            };
            let written = written_trait_ref(sigs, trait_ref.with_self(header[0]));
            let imp = &items.impls[later];
            diagnostics.push(Diagnostic {
                file: imp.file,
                range: imp.def_range,
                message: format!(
                    "conflicting implementations of trait `{written}` for type `{}`",
                    header[0]
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
/// rustc's E0367 for the lifetime bounds of `Drop` impl `imp` (`'b: 'a`,
/// `T: 'a`) that its struct, with lifetimes `theirs`, does not have or
/// imply; the impl's header is the struct's parameters, `args` its type
/// arguments.
fn drop_lifetime_bounds(
    imp: &flux_hir::Impl,
    regions: &ImplRegions,
    theirs: &StructRegions,
    args: &[Ty],
    error: &mut dyn FnMut(TextRange, &str),
) {
    // The impl's lifetime written as `l`, as the struct's.
    let lifetime = |l: flux_hir::LifetimeRef| -> Option<Region> {
        if l.is_static() {
            return Some(Region::Static);
        }
        let i = regions.names.iter().position(|n| n.name == Some(l.name))?;
        let k = regions
            .self_ty
            .iter()
            .position(|&r| r == Region::Param(i as u32))?;
        Some(Region::Param(k as u32))
    };
    // `a: b` among the struct's lifetimes, as its bounds and fields say.
    let known = |a: Region, b: Region| {
        let mut reached = vec![a];
        let mut i = 0;
        while let Some(&r) = reached.get(i) {
            i += 1;
            if r == b || r == Region::Static {
                return true;
            }
            for p in theirs.outlives.iter() {
                if let Outlives::Region(x, y) = *p
                    && x == r
                    && !reached.contains(&y)
                {
                    reached.push(y);
                }
            }
        }
        false
    };
    // rustc's span for a lifetime's bound is the header with its `where`
    // clause.
    let g = &imp.generics;
    let end = g
        .outlives
        .iter()
        .map(|p| p.shorter.range.end())
        .chain(g.type_outlives.iter().map(|p| p.lifetime.range.end()))
        .chain(g.predicates.iter().map(|p| p.bound.range.end()))
        .fold(imp.def_range.end(), |a, b| a.max(b));
    let whole = TextRange::new(imp.def_range.start(), end);
    for p in g.outlives.iter() {
        let (Some(a), Some(b)) = (lifetime(p.longer), lifetime(p.shorter)) else {
            continue;
        };
        if !known(a, b) {
            let message = format!(
                "`Drop` impl requires `{}: {}` but the struct it is implemented for does not",
                p.longer.name, p.shorter.name
            );
            error(whole, &message);
        }
    }
    for p in g.type_outlives.iter() {
        let TypeRef::Path(path) = &p.ty else { continue };
        let Some(name) = path.as_name() else { continue };
        let Some(j) = g.params.iter().position(|q| q.name == name) else {
            continue;
        };
        // The struct's parameter that the impl's `j` is.
        let is_param =
            |t: Ty, i: usize| matches!(t.kind(), TyKind::Param(q) if q.index as usize == i);
        let Some(k) = args.iter().position(|&a| is_param(a, j)) else {
            continue;
        };
        let Some(b) = lifetime(p.lifetime) else {
            continue;
        };
        let holds = theirs.outlives.iter().any(|q| match *q {
            Outlives::Type(t, c) => is_param(t, k) && known(c, b),
            Outlives::Region(..) => false,
        });
        if !holds {
            let message = format!(
                "`Drop` impl requires `{name}: {}` but the struct it is implemented for does not",
                p.lifetime.name
            );
            error(p.lifetime.range, &message);
        }
    }
}

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
        // Its lifetime arguments too: distinct parameters, not `'static`.
        let regions = sigs.regions.impls.get(id);
        let header = regions.map_or(&[][..], |r| &r.self_ty);
        let mut seen = Vec::new();
        let specialized = specialized
            || header.iter().any(|r| match r {
                Region::Param(i) if !seen.contains(i) => {
                    seen.push(*i);
                    false
                }
                Region::Erased => false,
                _ => true,
            });
        if specialized {
            error(imp.def_range, "`Drop` impls cannot be specialized");
            continue;
        }
        if let (Some(regions), Some(theirs)) = (regions, sigs.regions.structs.get(adt.strukt)) {
            drop_lifetime_bounds(imp, regions, theirs, args, &mut error);
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

#[cfg(test)]
mod coherence_tests {
    use super::*;

    #[test]
    fn unresolved_impl_header_does_not_prove_a_goal_impossible() {
        let (_, _, hir) = flux_hir::lower_text_bare(
            "incomplete.rs",
            "trait Bound {} struct Local; impl Bound for Local {}",
        );
        assert!(hir.diagnostics.is_empty());
        let (mut sigs, errors) = crate::signature::signatures(&hir.items);
        assert!(errors.is_empty());
        let (imp, _) = hir.items.impls.iter().next().expect("fixture impl");
        let trait_ref = sigs.impl_traits[imp];
        let self_ty = sigs.impls[imp];
        let own = hir.items.modules[hir.items.traits[trait_ref.trait_].loc.module].krate;
        sigs.impl_traits = Default::default();
        assert!(!coherence_goal_impossible(
            &hir.items,
            &sigs,
            own,
            Predicate { self_ty, trait_ref },
            &mut Vec::new()
        ));
    }
}
