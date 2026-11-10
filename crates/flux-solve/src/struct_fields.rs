//! The types of what values of a type hold: tuples' elements, structs'
//! fields and enums' variants' fields, with the associated types they
//! name normalized where their arguments make them known.

use std::cell::RefCell;
use std::marker::PhantomData;

use flux_hir::{ImplId, StructId, TraitId};
use rustc_hash::FxHashMap;

use crate::{ClosureArgs, Mutability, Predicate, Signatures, Ty, TyKind, max_param, traits, ty};

/// The types of the fields of `ty`, a tuple or struct; none for an
/// enum, whose fields are its variants'.
pub fn fields(sigs: &Signatures, ty: Ty) -> Option<&'static [Ty]> {
    match ty.kind() {
        TyKind::Tuple(elements) => Some(elements),
        TyKind::Adt(adt, _) if sigs.is_enum(adt.strukt) => None,
        TyKind::Adt(adt, args) => Some(adt_field_types(sigs, ty, adt.strukt, args)),
        TyKind::Closure(_, args) => Some(ClosureArgs::split(args).upvar_types()),
        _ => None,
    }
}

/// The types of the fields of variant `variant` of `ty`, an enum, or
/// of `ty` itself, a struct or tuple, for variant 0.
pub fn variant_fields(sigs: &Signatures, ty: Ty, variant: u32) -> Option<&'static [Ty]> {
    match ty.kind() {
        TyKind::Adt(adt, args) if sigs.is_enum(adt.strukt) => {
            let starts = &sigs.variant_starts[adt.strukt];
            let (start, end) = (
                *starts.get(variant as usize)? as usize,
                *starts.get(variant as usize + 1)? as usize,
            );
            Some(&adt_field_types(sigs, ty, adt.strukt, args)[start..end])
        }
        _ => fields(sigs, ty),
    }
}

/// The types of everything a value of `ty` may hold directly: a
/// tuple's elements, a struct's fields, every variant's fields of an
/// enum.
pub fn contents(sigs: &Signatures, ty: Ty) -> Option<&'static [Ty]> {
    match ty.kind() {
        TyKind::Tuple(elements) => Some(elements),
        TyKind::Adt(adt, args) => Some(adt_field_types(sigs, ty, adt.strukt, args)),
        TyKind::Closure(_, args) => Some(ClosureArgs::split(args).upvar_types()),
        _ => None,
    }
}

/// The variants of `ty`, an enum, by their fields' types.
pub fn variants(sigs: &Signatures, ty: Ty) -> Option<Vec<&'static [Ty]>> {
    let TyKind::Adt(adt, _) = ty.kind() else {
        return None;
    };
    let count = sigs.variant_starts.get(adt.strukt)?.len() - 1;
    (0..count as u32)
        .map(|v| variant_fields(sigs, ty, v))
        .collect()
}

/// The field types of the structs and enums asked for so far, by type,
/// for one set of signatures, while a [`FieldTypesScope`] is active.
struct Remembered {
    /// Identifies the signatures; never dereferenced.
    sigs: *const Signatures,
    fields: FxHashMap<Ty, &'static [Ty]>,
}

thread_local! {
    static REMEMBERED: RefCell<Option<Remembered>> = const { RefCell::new(None) };
}

/// Keeps the field types that [`fields`], [`variant_fields`] and
/// [`contents`] compute for one set of signatures on this thread until
/// dropped, then restores the scope active before. See
/// [`remember_field_types`].
pub struct FieldTypesScope<'a> {
    /// What to restore, unless a scope for the same signatures was
    /// already active and this one leaves it be.
    previous: Option<Option<Remembered>>,
    _sigs: PhantomData<&'a Signatures>,
}

/// Remembers the field types of the structs and enums asked for with
/// `sigs` until the scope drops: for the phases after type checking,
/// whose signatures are final. The scope borrows `sigs`, so they cannot
/// change while it is active.
pub fn remember_field_types(sigs: &Signatures) -> FieldTypesScope<'_> {
    let previous = REMEMBERED.with(|remembered| {
        let mut remembered = remembered.borrow_mut();
        if remembered
            .as_ref()
            .is_some_and(|r| std::ptr::eq(r.sigs, sigs))
        {
            return None;
        }
        Some(remembered.replace(Remembered {
            sigs,
            fields: FxHashMap::default(),
        }))
    });
    FieldTypesScope {
        previous,
        _sigs: PhantomData,
    }
}

impl Drop for FieldTypesScope<'_> {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            REMEMBERED.with(|remembered| *remembered.borrow_mut() = previous);
        }
    }
}

/// [`field_types`] of `ty`, `strukt` with arguments `args`, remembered
/// while a scope for `sigs` is active.
fn adt_field_types(sigs: &Signatures, ty: Ty, strukt: StructId, args: &[Ty]) -> &'static [Ty] {
    match with_remembered(sigs, |fields| fields.get(&ty).copied()) {
        Some(Some(found)) => found,
        Some(None) => {
            let computed = field_types(sigs, strukt, args);
            with_remembered(sigs, |fields| fields.insert(ty, computed));
            computed
        }
        None => field_types(sigs, strukt, args),
    }
}

/// Runs `f` on the field types remembered for `sigs`, if a scope for
/// them is active.
fn with_remembered<R>(
    sigs: &Signatures,
    f: impl FnOnce(&mut FxHashMap<Ty, &'static [Ty]>) -> R,
) -> Option<R> {
    REMEMBERED.with(|remembered| match &mut *remembered.borrow_mut() {
        Some(r) if std::ptr::eq(r.sigs, sigs) => Some(f(&mut r.fields)),
        _ => None,
    })
}

/// The types of the fields of `strukt` with generic arguments `args`,
/// with the associated types they name (`T::Out`) normalized where
/// `args` make them known.
pub fn field_types(sigs: &Signatures, strukt: StructId, args: &[Ty]) -> &'static [Ty] {
    let fields = sigs.structs[strukt];
    if args.is_empty() {
        return fields;
    }
    let fields = ty::map_list(fields, |t| t.subst(args));
    if !fields.iter().any(|f| f.has_aliases()) {
        return fields;
    }
    let (mut done, mut active) = (rustc_hash::FxHashMap::default(), Vec::new());
    ty::map_list(fields, |f| {
        normalize_known(sigs, f, 0, &mut done, &mut active)
    })
}

/// The generic arguments of `imp`, a block of `trait_`, for which its
/// header is `args` (the trait's `Self` and arguments), as far as the
/// header has them.
fn match_header(
    sigs: &Signatures,
    imp: ImplId,
    trait_: TraitId,
    args: &[Ty],
) -> Option<Vec<Option<Ty>>> {
    let trait_ref = sigs.impl_traits.get(imp).filter(|t| t.trait_ == trait_)?;
    // Every parameter of a trait's block is in its header, but those
    // only its bounds' associated types decide.
    let header = std::iter::once(sigs.impls[imp]).chain(trait_ref.args.iter().copied());
    let count = header.clone().map(max_param).max().unwrap_or(0);
    let count = count as usize;
    let mut short = [None; 8];
    let mut long = Vec::new();
    let binds = if count <= short.len() {
        &mut short[..count]
    } else {
        long.resize(count, None);
        &mut long[..]
    };
    header
        .zip(args)
        .all(|(x, &y)| traits::match_ty(x, y, binds))
        .then(|| binds.to_vec())
}

/// Whether the bounds of `imp`, with generic arguments `binds`, hold:
/// `Some(true)` if they surely do, `Some(false)` if they may (on
/// parameters, or on types with the compiler's own impls, which only
/// the items know), `None` if one does not.
fn bounds_hold(sigs: &Signatures, imp: ImplId, binds: &[Option<Ty>], depth: u32) -> Option<bool> {
    let preds = sigs.impl_predicates.get(imp).map_or(&[][..], |p| p);
    if preds.is_empty() {
        return Some(true);
    }
    let known: Vec<Ty> = binds.iter().map(|b| b.unwrap_or(Ty::ERROR)).collect();
    let mut definite = true;
    for p in preds {
        let p = p.subst(&known);
        definite &= may_hold(sigs, p, depth + 1)?;
    }
    Some(definite)
}

/// Whether `p` holds, as [`bounds_hold`] says: surely only if
/// an `impl` block whose bounds surely hold is for its types; perhaps
/// for parameters, which bounds in scope may say implement it, and
/// types with the compiler's own impls (arrays, closures, functions,
/// references' `Copy`), which only the items know; not at all if no
/// block may be for its types.
fn may_hold(sigs: &Signatures, p: Predicate, depth: u32) -> Option<bool> {
    let rigid = |t: Ty| {
        matches!(
            t.kind(),
            TyKind::Param(_) | TyKind::Alias(_) | TyKind::Opaque(_) | TyKind::Infer(_)
        )
    };
    if p.self_ty == Ty::ERROR || p.trait_ref.args.contains(&Ty::ERROR) {
        return Some(true);
    }
    if Some(p.trait_ref.trait_) == sigs.builtins.sized {
        return Some(!sigs.is_unsized(p.self_ty));
    }
    if rigid(p.self_ty) || depth > 8 {
        return Some(false);
    }
    // A trait object implements its trait and supertraits, with no
    // `impl` block (`dyn Iterator<Item = u8>: Iterator`).
    if let TyKind::Dynamic(d) = p.self_ty.kind()
        && traits::dyn_bounds(sigs, p.self_ty, d).contains(&p)
    {
        return Some(true);
    }
    if matches!(
        p.self_ty.kind(),
        TyKind::Array(..)
            | TyKind::Closure(..)
            | TyKind::FnDef(_)
            | TyKind::FnPtr(_)
            | TyKind::Ref(Mutability::Not, _)
            | TyKind::Never
    ) {
        return Some(false);
    }
    let mut args = vec![p.self_ty];
    args.extend_from_slice(p.trait_ref.args);
    let mut found = None;
    for imp in sigs.impl_index.matching_of(p.trait_ref.trait_, p.self_ty) {
        let Some(binds) = match_header(sigs, imp, p.trait_ref.trait_, &args) else {
            continue;
        };
        match bounds_hold(sigs, imp, &binds, depth) {
            Some(true) => return Some(true),
            Some(false) => found = Some(false),
            None => {}
        }
    }
    // Structural auto traits need not have a written impl. This query only
    // selects a field projection's candidate; it cannot disprove such a bound
    // from the absence of explicit blocks. The trait solver checks the bound.
    if found.is_none() && sigs.builtins.auto_traits.contains(&p.trait_ref.trait_) {
        return Some(false);
    }
    if found.is_none() && p.trait_ref.args.iter().any(|&a| rigid(a)) {
        return Some(false);
    }
    found
}

/// `ty` with each associated type of known types replaced by what
/// their trait's `impl` block for them says it is. `done` has what
/// the types met so far normalize to, and `active` the aliases being
/// normalized, as [`traits::normalize`] does.
fn normalize_known(
    sigs: &Signatures,
    ty: Ty,
    depth: u32,
    done: &mut rustc_hash::FxHashMap<Ty, Ty>,
    active: &mut Vec<Ty>,
) -> Ty {
    if !ty.has_aliases() || depth > 64 || active.contains(&ty) {
        return ty;
    }
    if let Some(&done) = done.get(&ty) {
        return done;
    }
    active.push(ty);
    let mut go = |t: Ty| normalize_known(sigs, t, depth + 1, done, active);
    let normalized = match ty.kind() {
        TyKind::Alias(alias) => 'alias: {
            let alias = alias.map_args(&mut go);
            let args = alias.args;
            // What a closure or function returns: `FnOnce`'s `Output`,
            // the only trait with an associated type they implement.
            if alias.name.as_str() == "Output" {
                if let Some(sig) = args[0].fn_sig()
                    && !sig.is_unsafe
                {
                    break 'alias go(sig.ret());
                }
                if let TyKind::Closure(_, closure) = args[0].kind() {
                    break 'alias go(ClosureArgs::split(closure).ret());
                }
            }
            // What a trait object says its trait's and supertraits'
            // associated types are (`dyn Iterator<Item = u8>`), and
            // what their traits imply.
            if let TyKind::Dynamic(d) = args[0].kind() {
                let bounds = traits::dyn_bounds(sigs, args[0], d);
                if bounds
                    .iter()
                    .any(|p| p.trait_ref.trait_ == alias.trait_ && p.trait_ref.args == &args[1..])
                    && let Some(t) = d.assoc(alias.name)
                {
                    break 'alias go(t);
                }
                if let Some(p) = traits::implied_projections(sigs, &bounds)
                    .into_iter()
                    .find(|p| p.alias == alias)
                {
                    break 'alias go(p.ty);
                }
            }
            // The one block whose header matches and whose bounds can
            // hold, as [`traits::select_impl`] picks it: one whose
            // bounds surely hold, else, for a type whose shape picks
            // it, the only one whose bounds may (on parameters).
            let mut judged = Vec::new();
            for imp in sigs.impl_index.matching_of(alias.trait_, args[0]) {
                let Some(binds) = match_header(sigs, imp, alias.trait_, args) else {
                    continue;
                };
                let Some(definite) = bounds_hold(sigs, imp, &binds, 0) else {
                    continue;
                };
                judged.push((imp, binds, definite));
            }
            let rigid = matches!(
                args[0].kind(),
                TyKind::Param(_) | TyKind::Alias(_) | TyKind::Opaque(_) | TyKind::Infer(_)
            );
            let definite = judged.iter().filter(|j| j.2).count();
            let chosen = match (definite, judged.len()) {
                (1, _) => judged.into_iter().find(|j| j.2),
                (0, 1) if !rigid => judged.into_iter().next(),
                _ => None,
            };
            if let Some((imp, binds, _)) = chosen
                && let Some(t) = traits::impl_type(sigs, imp, alias.name)
            {
                let mut binds = binds;
                // Parameters only the block's bounds' associated
                // types decide (`T` in `impl<'a, T, I: Iterator<Item
                // = &'a T>> Iterator for Copied<I>`), from what
                // those are for the known ones.
                let projections = sigs.impl_projections.get(imp).map_or(&[][..], |p| p);
                let rhs_owner_params = sigs
                    .impl_families
                    .get(imp)
                    .and_then(|families| families.iter().find(|(name, _)| *name == alias.name))
                    .map_or_else(|| max_param(t), |(_, family)| family.owner_params);
                let needed = projections
                    .iter()
                    .map(|p| max_param(p.ty))
                    .chain([rhs_owner_params])
                    .max()
                    .unwrap_or(0) as usize;
                if binds.len() < needed {
                    binds.resize(needed, None);
                }
                let mut progress = true;
                while progress && binds.iter().any(Option::is_none) {
                    progress = false;
                    for p in projections {
                        let known: Vec<Ty> = binds.iter().map(|b| b.unwrap_or(Ty::ERROR)).collect();
                        let projected = Ty::alias(p.alias).subst(&known);
                        if projected.has_params() || projected.has_infer() {
                            continue;
                        }
                        let ty = go(projected);
                        if matches!(ty.kind(), TyKind::Alias(_) | TyKind::Error) {
                            continue;
                        }
                        let mut found = binds.clone();
                        if traits::match_ty(p.ty, ty, &mut found) && found != binds {
                            binds = found;
                            progress = true;
                        }
                    }
                }
                let binds: Vec<Ty> = binds.into_iter().map(|b| b.unwrap_or(Ty::ERROR)).collect();
                if let Some(t) = traits::instantiate_impl_type(sigs, imp, alias, &binds) {
                    break 'alias go(t);
                }
            }
            Ty::alias(alias)
        }
        TyKind::Tuple(list) => Ty::new(TyKind::Tuple(ty::map_list(list, go))),
        TyKind::Adt(adt, list) => {
            let list = ty::map_list(list, go);
            Ty::new(TyKind::Adt(adt, list))
        }
        TyKind::Closure(id, list) => {
            let list = ty::map_list(list, go);
            Ty::new(TyKind::Closure(id, list))
        }
        TyKind::Ref(m, t) => Ty::new(TyKind::Ref(m, go(t))),
        TyKind::RawPtr(m, t) => Ty::raw_ptr(m, go(t)),
        TyKind::Array(t, len) => Ty::array_of(go(t), len),
        TyKind::Slice(t) => Ty::slice(go(t)),
        TyKind::FnDef(_) | TyKind::FnPtr(_) => ty.map_fn(go),
        _ => ty,
    };
    active.pop();
    done.insert(ty, normalized);
    normalized
}

/// The array and slice element types at matching structural tails.
/// Prefix fields must be unchanged by this built-in unsizing coercion.
pub fn array_unsize(sigs: &Signatures, from: Ty, to: Ty) -> Option<(Ty, Ty)> {
    let mut pair = (from, to);
    for _ in 0..256 {
        match (pair.0.kind(), pair.1.kind()) {
            (TyKind::Array(a, _), TyKind::Slice(b)) => return Some((a, b)),
            (TyKind::Adt(a, _), TyKind::Adt(b, _)) if a == b => {
                let (from, to) = (fields(sigs, pair.0)?, fields(sigs, pair.1)?);
                let last = from.len().checked_sub(1)?;
                if from.len() != to.len() || from[..last] != to[..last] {
                    return None;
                }
                pair = (from[last], to[last]);
            }
            _ => return None,
        }
    }
    None
}
