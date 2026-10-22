//! Trait objects: `dyn Tr` as a type, and what makes a trait usable as
//! one (rustc's *dyn compatibility*).
//!
//! A trait object's type ([`TyKind::Dynamic`]) is its trait, the
//! trait's generic arguments, and every associated type of the trait
//! and its supertraits, which must all be given (`dyn Iterator<Item =
//! u32>`). It implements its trait and their supertraits. Its lifetime
//! bound (`dyn Tr + 'a`) is a region slot, kept beside the type as
//! lifetimes are ([`crate::regions`]).
//!
//! Everything here reads the item tree alone, so it can be asked while
//! signatures are resolved: a trait's supertraits are found from their
//! paths, and dyn compatibility from its items as written.

use flux_hir::{Bound, DynRef, ItemTree, ModuleId, TraitId, TypeRef};
use flux_intern::{Symbol, kw};
use flux_syntax::TextRange;

use crate::ty::{DynTy, symbol_list};
use crate::{Ty, TyKind, TypeScope};

/// The `Self` a trait object's bounds are resolved with: no type, so
/// that a default naming `Self` shows (E0393).
fn object_self() -> Ty {
    Ty::param(u32::MAX, kw::SelfUpper)
}

/// Trait `t` and its supertraits, through any chain of them, each once,
/// `t` first, then depth first in the order they are written.
pub fn supertraits_of(items: &ItemTree, t: TraitId) -> Vec<TraitId> {
    let mut out = vec![t];
    let mut i = 0;
    while let Some(&current) = out.get(i) {
        i += 1;
        let data = &items.traits[current];
        let mut found = Vec::new();
        for bound in data.supertraits.iter() {
            if let (Some(sup), _) = items.resolve_trait(data.loc.module, bound)
                && !out.contains(&sup)
                && !found.contains(&sup)
            {
                found.push(sup);
            }
        }
        // `where Self: Sup` is a supertrait too.
        for p in data.generics.predicates.iter() {
            if is_self(&p.ty)
                && let (Some(sup), _) = items.resolve_trait(data.loc.module, &p.bound)
                && !out.contains(&sup)
                && !found.contains(&sup)
            {
                found.push(sup);
            }
        }
        out.splice(i..i, found);
    }
    out
}

/// The associated types a trait object of `t` gives, in order: each of
/// `t`'s and its supertraits' ([`supertraits_of`]), as each declares
/// them.
pub fn dyn_assoc(items: &ItemTree, t: TraitId) -> Vec<(TraitId, Symbol)> {
    supertraits_of(items, t)
        .into_iter()
        .flat_map(|t| items.traits[t].types.iter().map(move |ty| (t, ty.name)))
        .collect()
}

/// Whether `ty` is `Self`, as written.
fn is_self(ty: &TypeRef) -> bool {
    matches!(ty, TypeRef::Path(p) if p.as_name() == Some(kw::SelfUpper))
}

/// Whether `ty` names `Self` other than through its associated types
/// (`Self::Item` is allowed in a trait object's methods).
fn mentions_self(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Path(p) => {
            if p.qself.is_none() && p.segments.first() == Some(&kw::SelfUpper) {
                return p.segments.len() == 1;
            }
            p.args.iter().flatten().any(|a| {
                a.types.iter().any(mentions_self) || a.bindings.iter().any(|b| mentions_self(&b.ty))
            })
        }
        TypeRef::Tuple(list) => list.iter().any(mentions_self),
        TypeRef::Ptr { pointee, .. }
        | TypeRef::Ref { pointee, .. }
        | TypeRef::Array { elem: pointee, .. }
        | TypeRef::Slice(pointee) => mentions_self(pointee),
        TypeRef::FnPtr { params, ret, .. } => {
            params.iter().any(mentions_self) || mentions_self(ret)
        }
        TypeRef::Dyn(d) => d.bounds.iter().any(|b| {
            b.path.args.iter().flatten().any(|a| {
                a.types.iter().any(mentions_self) || a.bindings.iter().any(|b| mentions_self(&b.ty))
            })
        }),
        _ => false,
    }
}

/// Whether `bound`, written in `module`, names `core::marker::Sized`.
fn is_sized(items: &ItemTree, module: ModuleId, bound: &Bound) -> bool {
    matches!(
        items.resolve_trait(module, bound),
        (Some(t), _) if items.traits[t].lang == Some(flux_intern::sym::sized)
    )
}

/// Why trait `t` cannot be made a trait object, as rustc's note says
/// ("...because method `g` has generic type parameters"), if it cannot:
/// the first reason found in it or its supertraits.
pub fn dyn_incompatibility(items: &ItemTree, t: TraitId) -> Option<String> {
    for current in supertraits_of(items, t) {
        let data = &items.traits[current];
        let module = data.loc.module;
        let requires_sized = data.supertraits.iter().any(|b| is_sized(items, module, b))
            || data
                .generics
                .predicates
                .iter()
                .any(|p| is_self(&p.ty) && is_sized(items, module, &p.bound));
        if requires_sized {
            return Some("it requires `Self: Sized`".into());
        }
        // `trait Tr: PartialEq` is `PartialEq<Self>`.
        for bound in data.supertraits.iter() {
            let last = bound.path.segments.len().saturating_sub(1);
            let args = bound.path.args(last);
            let explicit = args.is_some_and(|a| a.types.iter().any(mentions_self));
            let defaulted = match items.resolve_trait(module, bound) {
                (Some(sup), _) => {
                    let given = args.map_or(0, |a| a.types.len());
                    items.traits[sup].generics.params
                        [given.min(items.traits[sup].generics.params.len())..]
                        .iter()
                        .any(|p| p.default.as_ref().is_some_and(|(d, _)| mentions_self(d)))
                }
                _ => false,
            };
            if explicit || defaulted {
                return Some("it uses `Self` as a type parameter".into());
            }
        }
        if let Some(&c) = data.consts.first() {
            let name = items.consts[c].name.map_or("_", |n| n.as_str());
            return Some(format!("it contains associated const `{name}`"));
        }
        for &f in &data.functions {
            let function = &items.functions[f];
            // `where Self: Sized` keeps a method off trait objects.
            if function
                .generics
                .predicates
                .iter()
                .any(|p| is_self(&p.ty) && is_sized(items, function.loc.module, &p.bound))
            {
                continue;
            }
            let name = function.name;
            if !function.has_self {
                return Some(format!(
                    "associated function `{name}` has no `self` parameter"
                ));
            }
            if !function.generics.params.is_empty() {
                return Some(format!("method `{name}` has generic type parameters"));
            }
            if function.params.iter().skip(1).any(|p| mentions_self(&p.ty)) {
                return Some(format!(
                    "method `{name}` references the `Self` type in this parameter"
                ));
            }
            if !function.opaques.is_empty() {
                return Some(format!(
                    "method `{name}` references an `impl Trait` type in its return type"
                ));
            }
            if mentions_self(&function.ret) {
                return Some(format!(
                    "method `{name}` references the `Self` type in its return type"
                ));
            }
        }
    }
    None
}

/// `core::ptr::drop_in_place`, if `core` has it: what a vtable's drop
/// function calls.
pub fn drop_in_place(items: &ItemTree) -> Option<flux_hir::FunctionId> {
    items.functions.iter().find_map(|(id, f)| {
        let in_ptr =
            items.is_core(f.loc.module) && items.modules[f.loc.module].name.as_str() == "ptr";
        (in_ptr && f.owner.is_none() && f.name.as_str() == "drop_in_place" && f.has_body)
            .then_some(id)
    })
}

/// Whether method `f` of a trait may be called on a trait object: it
/// does not say `where Self: Sized`.
pub fn is_dispatchable(items: &ItemTree, f: flux_hir::FunctionId) -> bool {
    let function = &items.functions[f];
    function.has_self
        && !function
            .generics
            .predicates
            .iter()
            .any(|p| is_self(&p.ty) && is_sized(items, function.loc.module, &p.bound))
}

/// The type `dyn Tr<..> + 'a` stands for, written in `module`: its
/// trait must be dyn compatible (E0038), give every associated type
/// (E0191), and be the only trait (E0225; flux has no auto traits).
pub(crate) fn resolve_dyn(
    items: &ItemTree,
    module: ModuleId,
    d: &DynRef,
    scope: TypeScope<'_>,
    range: TextRange,
    errors: &mut Vec<(TextRange, String)>,
) -> Ty {
    let fail = |errors: &mut Vec<(TextRange, String)>, at: TextRange, message: String| {
        errors.push((at, message));
        Ty::ERROR
    };
    let first = &d.bounds[0];
    let (t, error) = items.resolve_trait(module, first);
    if let Some(error) = error {
        errors.push(error);
    }
    let Some(t) = t else {
        return Ty::ERROR;
    };
    if let Some(extra) = d.bounds.get(1) {
        let (_, error) = items.resolve_trait(module, extra);
        if let Some(error) = error {
            errors.push(error);
            return Ty::ERROR;
        }
        let at = extra
            .path
            .range(extra.path.segments.len().saturating_sub(1), extra.range);
        return fail(
            errors,
            at,
            "only auto traits can be used as additional traits in a trait object".into(),
        );
    }
    let path = &first.path;
    let last = path.segments.len().saturating_sub(1);
    let name_range = path.range(last, first.range);
    // The path and its arguments, where rustc reports what they lack.
    let path_range = match (path.ranges.first(), path.args(last)) {
        (Some(start), Some(args)) => TextRange::new(start.start(), args.range.end()),
        (Some(start), None) => TextRange::new(start.start(), name_range.end()),
        _ => first.range,
    };
    let self_ty = object_self();
    let Some((trait_ref, projections)) =
        crate::traits::trait_ref(items, module, t, first, self_ty, scope, errors)
    else {
        return Ty::ERROR;
    };
    let data = &items.traits[t];
    for (i, &arg) in trait_ref.args.iter().enumerate() {
        if crate::mentions_param(arg, u32::MAX) {
            let param = data.generics.params[i].name;
            return fail(
                errors,
                name_range,
                format!("the type parameter `{param}` must be explicitly specified"),
            );
        }
    }
    if let Some(reason) = dyn_incompatibility(items, t) {
        // rustc points at the trait for its supertraits' `Self`.
        let at = if reason.contains("as a type parameter") {
            name_range
        } else {
            d.range
        };
        return fail(
            errors,
            at,
            format!("the trait `{}` is not dyn compatible", data.name),
        );
    }
    let assoc = dyn_assoc(items, t);
    let mut names = Vec::new();
    let mut tys = Vec::new();
    let mut missing = Vec::new();
    for (owner, name) in assoc {
        if names.contains(&name) {
            return fail(
                errors,
                d.range,
                "trait objects of traits with two associated types of one name are not supported yet"
                    .into(),
            );
        }
        let given = projections
            .iter()
            .find(|p| p.alias.name == name && p.alias.trait_ == owner)
            .or_else(|| projections.iter().find(|p| p.alias.name == name));
        match given {
            Some(p) => {
                names.push(name);
                tys.push(p.ty);
            }
            None => missing.push(name),
        }
    }
    if !missing.is_empty() {
        let list: Vec<String> = missing.iter().map(|n| format!("`{n}`")).collect();
        let (kind, list) = match list.as_slice() {
            [one] => ("type", one.clone()),
            [init @ .., last] => ("types", format!("{} and {last}", init.join(", "))),
            [] => unreachable!(),
        };
        let at = if path.args(last).is_some() {
            path_range
        } else {
            name_range
        };
        return fail(
            errors,
            at,
            format!(
                "the value of the associated {kind} {list} in `{}` must be specified",
                data.name
            ),
        );
    }
    if tys.contains(&Ty::ERROR) {
        return Ty::ERROR;
    }
    // A reference in its arguments would need region slots of its own.
    if trait_ref
        .args
        .iter()
        .chain(&tys)
        .any(|&ty| crate::regions::slots(items, ty) > 0 || mentions_object(ty))
    {
        return fail(
            errors,
            range,
            "references and trait objects in trait objects' arguments are not supported yet".into(),
        );
    }
    Ty::new(TyKind::Dynamic(DynTy {
        trait_: t,
        name: data.name,
        args: trait_ref.args,
        assoc_names: symbol_list(&names),
        assoc_tys: Ty::list(&tys),
    }))
}

/// Whether `ty` contains a trait object.
fn mentions_object(ty: Ty) -> bool {
    ty.is_dyn() || ty.children().into_iter().any(mentions_object)
}
