//! Virtual tables: what a pointer to a trait object carries besides its
//! data, and what a call of one of its trait's methods calls.
//!
//! A trait object's vtable is, as rustc's, three words (the function
//! that drops a value of the type, its size and its alignment), then a
//! word per *slot*: for each supertrait of its trait but the trait
//! itself (each one [`flux_ty::traits::dyn_bounds`] lists), a pointer to
//! the type's vtable as that supertrait's object, which upcasting loads;
//! then, for the trait and each supertrait in that order, each of its
//! methods in turn, the type's implementation of it (its `impl` block's,
//! or the trait's default), or nothing for one a trait object cannot
//! call (`where Self: Sized`). The layout depends only on the trait
//! object's type, so a call through any value of it finds a method at
//! the same slot.

use flux_hir::{FunctionId, ItemTree, Owner};
use flux_ty::traits::Predicate;
use flux_ty::{DynTy, Instance, Signatures, Ty, TyKind};

/// The words before the slots: drop, size, align.
pub const HEADER: usize = 3;

/// One slot of a trait object's vtable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    /// The vtable for this supertrait's object.
    Super(Predicate),
    /// Method `f` of the trait of this bound (the trait object's own
    /// trait or a supertrait, with their arguments).
    Method(Predicate, FunctionId),
}

/// What one slot of a vtable for a type holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    /// The address of the type's vtable as this trait object.
    Super(Ty),
    /// The address of this instance.
    Method(Instance),
    /// Nothing: a method a trait object cannot call.
    Null,
}

fn slots(items: &ItemTree, sigs: &Signatures, object: Ty, d: DynTy) -> Vec<Slot> {
    let bounds = flux_ty::traits::dyn_bounds(sigs, object, d);
    let mut out: Vec<Slot> = bounds.iter().skip(1).map(|&p| Slot::Super(p)).collect();
    for &p in &bounds {
        for &f in &items.traits[p.trait_ref.trait_].functions {
            if items.functions[f].has_self {
                out.push(Slot::Method(p, f));
            }
        }
    }
    out
}

/// The trait object `object` is, if it is one.
pub fn as_dyn(object: Ty) -> Option<DynTy> {
    match object.kind() {
        TyKind::Dynamic(d) => Some(d),
        _ => None,
    }
}

/// The type of the object of supertrait bound `p` of trait object
/// `object`, `d`: its trait and arguments, and the associated types of
/// it and its supertraits that `object` gives.
pub fn super_object(items: &ItemTree, sigs: &Signatures, object: Ty, d: DynTy, p: Predicate) -> Ty {
    let given = flux_ty::traits::dyn_projections(sigs, object, d);
    let mut names = Vec::new();
    let mut tys = Vec::new();
    for (_, name) in flux_ty::objects::dyn_assoc(items, p.trait_ref.trait_) {
        if let Some(q) = given.iter().find(|q| q.alias.name == name) {
            names.push(name);
            tys.push(q.ty);
        }
    }
    Ty::new(TyKind::Dynamic(DynTy {
        trait_: p.trait_ref.trait_,
        name: p.trait_ref.name,
        args: p.trait_ref.args,
        assoc_names: flux_ty::symbol_list(&names),
        assoc_tys: Ty::list(&tys),
    }))
}

/// The slots of the vtable for `concrete` as trait object `object`.
pub fn entries(items: &ItemTree, sigs: &Signatures, object: Ty, concrete: Ty) -> Vec<Entry> {
    let Some(d) = as_dyn(object) else {
        return Vec::new();
    };
    slots(items, sigs, object, d)
        .into_iter()
        .map(|slot| match slot {
            Slot::Super(p) => Entry::Super(super_object(items, sigs, object, d, p)),
            Slot::Method(p, f) => {
                if !flux_ty::objects::is_dispatchable(items, f)
                    || flux_ty::traits::self_by_value(items, f)
                {
                    return Entry::Null;
                }
                let mut args = vec![concrete];
                args.extend_from_slice(p.trait_ref.args);
                let instance = Instance {
                    function: f,
                    args: Ty::list(&args),
                };
                Entry::Method(flux_ty::traits::resolve_instance(items, sigs, instance))
            }
        })
        .collect()
}

/// What a vtable's first word, the function that drops a value of
/// `concrete` in place, calls: `core::ptr::drop_in_place::<T>`, where
/// dropping one runs code and `core` has it; `None` (an empty word)
/// otherwise.
pub fn drop_entry(items: &ItemTree, sigs: &Signatures, concrete: Ty) -> Option<Instance> {
    if !flux_ty::traits::needs_drop(items, sigs, &[], concrete) {
        return None;
    }
    let function = flux_ty::objects::drop_in_place(items)?;
    Some(Instance {
        function,
        args: Ty::list(&[concrete]),
    })
}

/// The index, in words, of method `f` in vtables of trait object
/// `object`, header included.
pub fn method_index(
    items: &ItemTree,
    sigs: &Signatures,
    object: Ty,
    f: FunctionId,
) -> Option<usize> {
    let d = as_dyn(object)?;
    let i = slots(items, sigs, object, d)
        .iter()
        .position(|s| matches!(s, Slot::Method(_, g) if *g == f))?;
    Some(HEADER + i)
}

/// The index, in words, of the vtable of supertrait object `target` in
/// vtables of trait object `object`, header included.
pub fn super_index(items: &ItemTree, sigs: &Signatures, object: Ty, target: Ty) -> Option<usize> {
    let d = as_dyn(object)?;
    let t = as_dyn(target)?;
    let i = slots(items, sigs, object, d)
        .iter()
        .position(|s| matches!(s, Slot::Super(p) if p.trait_ref.trait_ == t.trait_))?;
    Some(HEADER + i)
}

/// The trait object a call of `instance` dispatches on, if it is a
/// virtual call: a method of a trait whose `Self` is an object of it or
/// of a subtrait.
pub fn virtual_call(items: &ItemTree, instance: Instance) -> Option<Ty> {
    let Some(Owner::Trait(t)) = items.functions[instance.function].owner else {
        return None;
    };
    let object = *instance.args.first()?;
    let d = as_dyn(object)?;
    flux_ty::objects::supertraits_of(items, d.trait_)
        .contains(&t)
        .then_some(object)
}
