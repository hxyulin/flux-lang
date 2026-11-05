//! What makes a trait usable as a trait object (rustc's *dyn
//! compatibility*), and the supertraits and associated types an object
//! of it has.
//!
//! Everything here reads the item tree alone, so it can be asked while
//! signatures are resolved: a trait's supertraits are found from their
//! paths, and dyn compatibility from its items as written.

use flux_hir::def_map::{Def, Ns, Resolved};
use flux_hir::{Bound, ItemTree, ModuleId, TraitId, TypeRef};
use flux_intern::{Symbol, kw};

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
        .flat_map(|t| {
            let module = items.traits[t].loc.module;
            items.traits[t]
                .types
                .iter()
                .filter(move |ty| {
                    !ty.generics.predicates.iter().any(|predicate| {
                        is_self(&predicate.ty) && is_sized(items, module, &predicate.bound)
                    })
                })
                .map(move |ty| (t, ty.name))
        })
        .collect()
}

/// Whether `ty` is `Self`, as written.
fn is_self(ty: &TypeRef) -> bool {
    matches!(ty, TypeRef::Path(p) if p.as_name() == Some(kw::SelfUpper))
}

/// Whether a method whose receiver is `ty`, written in `module`, can be
/// called on a trait object: its receiver is `self`, or `Self` behind
/// `&`, `&mut`, `Box`, `Rc` or `Arc`, or behind a `Pin` of one of those
/// (rustc's `DispatchFromDyn`). Paths that name no struct are left to the
/// signature's receiver check.
fn dispatchable_receiver(items: &ItemTree, module: ModuleId, ty: &TypeRef, pinned: bool) -> bool {
    match ty {
        TypeRef::Ref { pointee, .. } => is_self(pointee),
        TypeRef::Path(path) if !is_self(ty) => {
            let last = path.segments.len().saturating_sub(1);
            let Some([arg]) = path.args(last).map(|args| &args.types[..]) else {
                return true;
            };
            let Ok(Resolved::Def(Def::Struct(strukt))) = items.resolve(module, path, Ns::Types)
            else {
                return true;
            };
            if Some(strukt) == flux_hir::owned_box()
                || crate::is_lang_struct(items, strukt, "arc")
                || crate::is_lang_struct(items, strukt, "rc")
            {
                is_self(arg)
            } else if !pinned && crate::is_lang_struct(items, strukt, "pin") {
                dispatchable_receiver(items, module, arg, true)
            } else {
                false
            }
        }
        _ => !pinned,
    }
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

/// Whether `bound`, written in `module`, requires a sized Self, directly
/// or through its supertraits.
fn is_sized(items: &ItemTree, module: ModuleId, bound: &Bound) -> bool {
    let (Some(trait_), _) = items.resolve_trait(module, bound) else {
        return false;
    };
    supertraits_of(items, trait_)
        .iter()
        .any(|&id| items.traits[id].lang == Some(flux_intern::sym::sized))
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
        for family in &data.types {
            if (!family.generics.params.is_empty() || !family.generics.lifetimes.is_empty())
                && !family.generics.predicates.iter().any(|predicate| {
                    is_self(&predicate.ty) && is_sized(items, module, &predicate.bound)
                })
            {
                return Some(format!(
                    "associated type `{}` has generic parameters",
                    family.name
                ));
            }
        }
        // `trait Tr: PartialEq` is `PartialEq<Self>`, as is
        // `where Self: PartialEq`.
        let where_self = data
            .generics
            .predicates
            .iter()
            .filter(|p| is_self(&p.ty))
            .map(|p| &p.bound);
        for bound in data.supertraits.iter().chain(where_self) {
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
        // `core`'s stand-ins for specialization are not rustc's consts.
        if let Some(&c) = data
            .consts
            .iter()
            .find(|&&c| !items.consts[c].specialization)
        {
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
            if let Some(receiver) = function.params.first()
                && !dispatchable_receiver(items, function.loc.module, &receiver.ty, false)
            {
                return Some(format!(
                    "method `{name}`'s `self` parameter cannot be dispatched on"
                ));
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
