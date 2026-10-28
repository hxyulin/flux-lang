//! Type inference and checking.
//!
//! Signatures come first, from the item tree alone ([`signatures`]); then
//! each body is inferred on its own against them ([`infer_body`]), so
//! bodies can be checked in parallel and an edit to one body does not
//! affect another's types.
//!
//! Inference is unification over type variables. An unsuffixed integer
//! literal gets an *integer* variable that only unifies with integer types
//! and defaults to `i32`. `!` coerces to any type instead of unifying, and
//! a variable fed only by `!` falls back to `!`, as in edition 2024.
//! Checks that need final types (literal ranges, operand types, casts)
//! run once the body is inferred.
//!
//! This is also where flux makes the checks rustc makes before borrow
//! checking: calls to `unsafe` functions need an `unsafe` block, and
//! assignments need a `mut` binding.

mod signature;
pub use signature::{signatures, signatures_checked};

mod resolve;
pub use resolve::{
    ArgTys, ArrayLens, LenEvaluator, LenValues, TypeScope, alias_type, arc_pointee, array_len,
    box_pointee, check_const_params, const_param_ty, const_value, generic_consts, generic_names,
    impl_target, is_lang_struct, maybe_unsized, nonzero_integer, param_const_ty, primitive,
    resolve_generic_arg, resolve_generic_args, resolve_type, self_ty, struct_type, unsized_error,
    unsized_params, wrong_arg_count,
};
use resolve::{
    alias_cycle, check_type_defaults, reaches_alias, resolve_or_report, resolve_predicates,
    unsized_argument,
};
pub(crate) use resolve::{contains_ref, expand_alias, fill_type_defaults, type_arg_count};

mod fn_regions;
mod impl_index;
mod in_scope;
mod infer;
mod metadata;
pub mod objects;
mod pats;
pub mod regions;
pub mod traits;
mod ty;

use std::fmt::Write;
use std::ops::Index;
use std::sync::OnceLock;

use flux_arena::{ArenaMap, LayeredArenaMap};
use flux_hir::def_map::{Def, Ns, ResolveError, Resolved};
use flux_hir::{
    Const, ConstId, Core, Diagnostic, Function, FunctionId, Generics, Hir, Impl, ImplId,
    ImplTarget, ItemCounts, ItemLoc, ItemTree, ModuleId, Owner, Path, Struct, StructId, Trait,
    TypeRef,
};
use flux_intern::{Symbol, kw, sym};
use flux_syntax::TextRange;
use rustc_hash::FxHashMap;

pub use infer::{
    BuiltinMethod, Capture, CaptureMode, CaptureProj, ClosureInfo, InferenceResult, Method,
    OverloadedDeref, c_char_ptr, c_str_ref, infer_body, infer_const,
};
pub use metadata::Metadata;
pub use traits::{Predicate, Projection, TraitRef, TraitSig};
pub use ty::{
    AdtId, AliasTy, ClosureArgs, ClosureId, ClosureKind, ClosureOwner, DynTy, FloatTy, FnDefTy,
    FnPtrRegions, FnRegion, FnSigTy, GenericArgs, InferVar, IntTy, Mutability, OpaqueTy, ParamTy,
    Ty, TyKind, subst_list, symbol_list,
};

/// Which `Fn`-family trait `t` is, if it is one: `core`'s
/// `#[lang = "fn"]`, `"fn_mut"` or `"fn_once"`.
pub fn fn_trait_kind(items: &ItemTree, t: flux_hir::TraitId) -> Option<ClosureKind> {
    match items.traits[t].lang?.as_str() {
        "fn" => Some(ClosureKind::Fn),
        "fn_mut" => Some(ClosureKind::FnMut),
        "fn_once" => Some(ClosureKind::FnOnce),
        _ => None,
    }
}

/// The `Fn`-family trait of `kind`, if `core` has it.
pub fn fn_trait(items: &ItemTree, kind: ClosureKind) -> Option<flux_hir::TraitId> {
    let lang = match kind {
        ClosureKind::Fn => "fn",
        ClosureKind::FnMut => "fn_mut",
        ClosureKind::FnOnce => "fn_once",
    };
    items.lang_traits.get(&Symbol::intern(lang)).copied()
}

/// A function's resolved signature. A generic function's mentions its
/// parameters, its `impl` block's first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnSig {
    pub params: Box<[Ty]>,
    pub ret: Ty,
}

/// A function with generic arguments for its parameters, its `impl`
/// block's first: what a call calls, and, once monomorphized, a function
/// in the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Instance {
    pub function: FunctionId,
    pub args: GenericArgs,
}

/// A constant with generic arguments for its owner's parameters: a
/// trait's, `Self` first, or its `impl` block's. What a path to an
/// associated constant names; which value it is follows once the
/// arguments are known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstInstance {
    pub konst: ConstId,
    pub args: GenericArgs,
}

impl FnSig {
    /// The signature of the instance with generic arguments `args`.
    pub fn subst(&self, args: &[Ty]) -> FnSig {
        FnSig {
            params: self.params.iter().map(|p| p.subst(args)).collect(),
            ret: self.ret.subst(args),
        }
    }
}

/// Everything about a file's items that bodies depend on: each
/// function's signature, each struct's field types, each `impl` block's
/// type and trait, and the bounds of each item, in terms of their
/// generic parameters.
#[derive(Debug, Clone, Default)]
pub struct Signatures {
    pub unions: rustc_hash::FxHashSet<StructId>,
    pub fns: LayeredArenaMap<Function, FnSig>,
    /// The types of each struct's fields, and of each enum's, every
    /// variant's in turn: all that a value of it may contain.
    pub structs: LayeredArenaMap<Struct, &'static [Ty]>,
    /// Where each variant of an enum starts in its `structs` entry, and
    /// where the last ends.
    pub variant_starts: LayeredArenaMap<Struct, Box<[u32]>>,
    /// The type of each enum's discriminant: [`discriminant_ty`].
    pub discriminants: LayeredArenaMap<Struct, IntTy>,
    pub impls: LayeredArenaMap<Impl, Ty>,
    pub consts: LayeredArenaMap<Const, Ty>,
    pub traits: LayeredArenaMap<Trait, TraitSig>,
    /// The trait each trait's `impl` block implements, if it resolved.
    pub impl_traits: LayeredArenaMap<Impl, TraitRef>,
    /// The bounds of each `impl` block, trait and function, their own
    /// only: a function's are after its owner's.
    pub impl_predicates: LayeredArenaMap<Impl, Box<[Predicate]>>,
    /// The bounds of each struct, which every type naming it must meet.
    pub struct_predicates: LayeredArenaMap<Struct, Box<[Predicate]>>,
    pub trait_predicates: LayeredArenaMap<Trait, Box<[Predicate]>>,
    pub fn_predicates: LayeredArenaMap<Function, Box<[Predicate]>>,
    /// What the bounds of each `impl` block, trait and function say
    /// associated types are (`T: Tr<Out = i32>`), their own only.
    pub impl_projections: LayeredArenaMap<Impl, Box<[Projection]>>,
    pub struct_projections: LayeredArenaMap<Struct, Box<[Projection]>>,
    pub trait_projections: LayeredArenaMap<Trait, Box<[Projection]>>,
    pub fn_projections: LayeredArenaMap<Function, Box<[Projection]>>,
    /// The associated types each trait's `impl` block defines, by name.
    pub impl_types: LayeredArenaMap<Impl, Box<[(Symbol, Ty)]>>,
    pub impl_families: LayeredArenaMap<Impl, Box<[(Symbol, traits::AssociatedTypeSig)]>>,
    /// The bounds of each function's opaque types (the `impl Trait`s of
    /// its return type), by index: [`traits::opaque_bounds`].
    pub opaques: LayeredArenaMap<Function, Box<[OpaqueSig]>>,
    /// The hidden type of each opaque type, in terms of its function's
    /// generic parameters, once the function's body is inferred: what
    /// code generation sees ([`traits::reveal`]).
    pub hidden: rustc_hash::FxHashMap<(FunctionId, u32), Ty>,
    /// The defaults of each trait's parameters.
    pub trait_defaults: traits::TraitDefaults,
    /// The `impl` blocks of each trait by the constructor of their type.
    pub impl_index: impl_index::ImplIndex,
    /// The traits in scope in each module.
    pub traits_in_scope: in_scope::TraitsInScope,
    /// A trait is its own supertrait (reported): rustc stops before
    /// type checking bodies.
    pub cyclic: bool,
    /// The values of the constants that array lengths name, evaluated
    /// before bodies are inferred.
    pub array_lens: LenValues,
    /// The types of the anonymous constants of const generic arguments
    /// (`f::<{ N + 1 }>`), their parameters'.
    pub arg_tys: ArgTys,
    /// The lifetimes in the signatures, for the borrow checker.
    pub regions: regions::RegionSigs,
    /// What field types need of the items to normalize without them.
    pub builtins: Builtins,
}

/// What an opaque type is known to be: its bounds and what they say its
/// associated types are, in terms of its function's generic parameters
/// and, for itself, one more parameter after them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpaqueSig {
    pub predicates: Box<[Predicate]>,
    pub projections: Box<[Projection]>,
}

/// The traits of `core` that [`Signatures::field_types`] needs to know,
/// recorded from the items: `Sized`, which every type implements.
#[derive(Debug, Clone, Copy, Default)]
pub struct Builtins {
    pub sized: Option<flux_hir::TraitId>,
}

impl Builtins {
    fn of(items: &ItemTree) -> Builtins {
        Builtins {
            sized: items.lang_traits.get(&flux_intern::sym::sized).copied(),
        }
    }
}

impl Index<FunctionId> for Signatures {
    type Output = FnSig;

    fn index(&self, function: FunctionId) -> &FnSig {
        &self.fns[function]
    }
}

impl Signatures {
    /// The lookup indexes are rebuilt for the extended item tree.
    fn fork_for_extension(&self) -> Self {
        Self {
            unions: self.unions.clone(),
            fns: self.fns.clone(),
            structs: self.structs.clone(),
            variant_starts: self.variant_starts.clone(),
            discriminants: self.discriminants.clone(),
            impls: self.impls.clone(),
            consts: self.consts.clone(),
            traits: self.traits.clone(),
            impl_traits: self.impl_traits.clone(),
            impl_predicates: self.impl_predicates.clone(),
            struct_predicates: self.struct_predicates.clone(),
            trait_predicates: self.trait_predicates.clone(),
            fn_predicates: self.fn_predicates.clone(),
            impl_projections: self.impl_projections.clone(),
            struct_projections: self.struct_projections.clone(),
            trait_projections: self.trait_projections.clone(),
            fn_projections: self.fn_projections.clone(),
            impl_types: self.impl_types.clone(),
            impl_families: self.impl_families.clone(),
            opaques: self.opaques.clone(),
            hidden: self.hidden.clone(),
            trait_defaults: self.trait_defaults.clone(),
            impl_index: Default::default(),
            traits_in_scope: Default::default(),
            cyclic: self.cyclic,
            array_lens: self.array_lens.clone(),
            arg_tys: self.arg_tys.clone(),
            regions: self.regions.clone(),
            builtins: self.builtins,
        }
    }

    /// Freeze the library's maps once; copies retain a shared base and
    /// clone individual inherited values only when they are changed.
    fn freeze(&mut self) {
        self.fns.freeze();
        self.structs.freeze();
        self.variant_starts.freeze();
        self.discriminants.freeze();
        self.impls.freeze();
        self.consts.freeze();
        self.traits.freeze();
        self.impl_traits.freeze();
        self.impl_predicates.freeze();
        self.struct_predicates.freeze();
        self.trait_predicates.freeze();
        self.fn_predicates.freeze();
        self.impl_projections.freeze();
        self.struct_projections.freeze();
        self.trait_projections.freeze();
        self.fn_projections.freeze();
        self.impl_types.freeze();
        self.impl_families.freeze();
        self.opaques.freeze();
        self.trait_defaults.freeze();
        self.regions.traits.freeze();
        self.regions.fns.freeze();
        self.regions.structs.freeze();
        self.regions.impls.freeze();
        self.regions.consts.freeze();
    }

    /// The types of the fields of `ty`, a tuple or struct; none for an
    /// enum, whose fields are its variants'.
    pub fn fields(&self, ty: Ty) -> Option<&'static [Ty]> {
        match ty.kind() {
            TyKind::Tuple(elements) => Some(elements),
            TyKind::Adt(adt, _) if self.is_enum(adt.strukt) => None,
            TyKind::Adt(adt, args) => Some(self.field_types(adt.strukt, args)),
            TyKind::Closure(_, args) => Some(ClosureArgs::split(args).upvar_types()),
            _ => None,
        }
    }

    /// The type of the discriminant of `ty`, an enum.
    pub fn discriminant_ty(&self, ty: Ty) -> Option<IntTy> {
        match ty.kind() {
            TyKind::Adt(adt, _) => self.discriminants.get(adt.strukt).copied(),
            _ => None,
        }
    }

    /// Whether `strukt` is an enum.
    pub fn is_enum(&self, strukt: StructId) -> bool {
        self.variant_starts.get(strukt).is_some()
    }

    /// The types of the fields of variant `variant` of `ty`, an enum, or
    /// of `ty` itself, a struct or tuple, for variant 0.
    pub fn variant_fields(&self, ty: Ty, variant: u32) -> Option<&'static [Ty]> {
        match ty.kind() {
            TyKind::Adt(adt, args) if self.is_enum(adt.strukt) => {
                let starts = &self.variant_starts[adt.strukt];
                let (start, end) = (
                    *starts.get(variant as usize)? as usize,
                    *starts.get(variant as usize + 1)? as usize,
                );
                Some(&self.field_types(adt.strukt, args)[start..end])
            }
            _ => self.fields(ty),
        }
    }

    /// The types of everything a value of `ty` may hold directly: a
    /// tuple's elements, a struct's fields, every variant's fields of an
    /// enum.
    pub fn contents(&self, ty: Ty) -> Option<&'static [Ty]> {
        match ty.kind() {
            TyKind::Tuple(elements) => Some(elements),
            TyKind::Adt(adt, args) => Some(self.field_types(adt.strukt, args)),
            TyKind::Closure(_, args) => Some(ClosureArgs::split(args).upvar_types()),
            _ => None,
        }
    }

    /// The variants of `ty`, an enum, by their fields' types.
    pub fn variants(&self, ty: Ty) -> Option<Vec<&'static [Ty]>> {
        let TyKind::Adt(adt, _) = ty.kind() else {
            return None;
        };
        let count = self.variant_starts.get(adt.strukt)?.len() - 1;
        (0..count as u32)
            .map(|v| self.variant_fields(ty, v))
            .collect()
    }

    /// The types of the fields of `strukt` with generic arguments `args`,
    /// with the associated types they name (`T::Out`) normalized where
    /// `args` make them known.
    pub fn field_types(&self, strukt: StructId, args: &[Ty]) -> &'static [Ty] {
        let fields = self.structs[strukt];
        if args.is_empty() {
            return fields;
        }
        let fields = ty::map_list(fields, |t| t.subst(args));
        if !fields.iter().any(|f| f.has_aliases()) {
            return fields;
        }
        let (mut done, mut active) = (rustc_hash::FxHashMap::default(), Vec::new());
        ty::map_list(fields, |f| {
            self.normalize_known(f, 0, &mut done, &mut active)
        })
    }

    /// The generic arguments of `imp` for which its header is `args`
    /// (the trait's `Self` and arguments), as far as the header has them.
    fn match_header(&self, imp: ImplId, args: &[Ty]) -> Option<Vec<Option<Ty>>> {
        let trait_ref = self.impl_traits.get(imp)?;
        // Every parameter of a trait's block is in its header, but those
        // only its bounds' associated types decide.
        let header = std::iter::once(self.impls[imp]).chain(trait_ref.args.iter().copied());
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
    fn bounds_hold(&self, imp: ImplId, binds: &[Option<Ty>], depth: u32) -> Option<bool> {
        let preds = self.impl_predicates.get(imp).map_or(&[][..], |p| p);
        if preds.is_empty() {
            return Some(true);
        }
        let known: Vec<Ty> = binds.iter().map(|b| b.unwrap_or(Ty::ERROR)).collect();
        let mut definite = true;
        for p in preds {
            let p = p.subst(&known);
            definite &= self.may_hold(p, depth + 1)?;
        }
        Some(definite)
    }

    /// Whether `p` holds, as [`Self::bounds_hold`] says: surely only if
    /// an `impl` block whose bounds surely hold is for its types; perhaps
    /// for parameters, which bounds in scope may say implement it, and
    /// types with the compiler's own impls (arrays, closures, functions,
    /// references' `Copy`), which only the items know; not at all if no
    /// block may be for its types.
    fn may_hold(&self, p: Predicate, depth: u32) -> Option<bool> {
        let rigid = |t: Ty| {
            matches!(
                t.kind(),
                TyKind::Param(_) | TyKind::Alias(_) | TyKind::Opaque(_) | TyKind::Infer(_)
            )
        };
        if p.self_ty == Ty::ERROR || p.trait_ref.args.contains(&Ty::ERROR) {
            return Some(true);
        }
        if Some(p.trait_ref.trait_) == self.builtins.sized {
            return Some(!self.is_unsized(p.self_ty));
        }
        if rigid(p.self_ty) || depth > 8 {
            return Some(false);
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
        for (imp, trait_ref) in self.impl_traits.iter() {
            if trait_ref.trait_ != p.trait_ref.trait_ {
                continue;
            }
            let Some(binds) = self.match_header(imp, &args) else {
                continue;
            };
            match self.bounds_hold(imp, &binds, depth) {
                Some(true) => return Some(true),
                Some(false) => found = Some(false),
                None => {}
            }
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
        &self,
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
        let mut go = |t: Ty| self.normalize_known(t, depth + 1, done, active);
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
                // The one block whose header matches and whose bounds can
                // hold, as [`traits::select_impl`] picks it: one whose
                // bounds surely hold, else, for a type whose shape picks
                // it, the only one whose bounds may (on parameters).
                let mut judged = Vec::new();
                for (imp, trait_ref) in self.impl_traits.iter() {
                    if trait_ref.trait_ != alias.trait_ {
                        continue;
                    }
                    let Some(binds) = self.match_header(imp, args) else {
                        continue;
                    };
                    let Some(definite) = self.bounds_hold(imp, &binds, 0) else {
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
                    && let Some(t) = traits::impl_type(self, imp, alias.name)
                {
                    let mut binds = binds;
                    // Parameters only the block's bounds' associated
                    // types decide (`T` in `impl<'a, T, I: Iterator<Item
                    // = &'a T>> Iterator for Copied<I>`), from what
                    // those are for the known ones.
                    let projections = self.impl_projections.get(imp).map_or(&[][..], |p| p);
                    let rhs_owner_params = self
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
                            let known: Vec<Ty> =
                                binds.iter().map(|b| b.unwrap_or(Ty::ERROR)).collect();
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
                    let binds: Vec<Ty> =
                        binds.into_iter().map(|b| b.unwrap_or(Ty::ERROR)).collect();
                    if let Some(t) = traits::instantiate_impl_type(self, imp, alias, &binds) {
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
}

/// One more than the largest index of a generic parameter in `ty`, or 0.
fn max_param(ty: Ty) -> u32 {
    match ty.kind() {
        TyKind::Param(p) => p.index + 1,
        TyKind::Tuple(list) | TyKind::Adt(_, list) | TyKind::Closure(_, list) => {
            list.iter().map(|&t| max_param(t)).max().unwrap_or(0)
        }
        TyKind::Alias(alias) => alias
            .args
            .iter()
            .chain(alias.family_args)
            .map(|&t| max_param(t))
            .max()
            .unwrap_or(0),
        TyKind::Opaque(opaque) => opaque.args.iter().map(|&t| max_param(t)).max().unwrap_or(0),
        TyKind::Ref(_, t) | TyKind::RawPtr(_, t) | TyKind::Slice(t) => max_param(t),
        TyKind::Array(t, len) => max_param(t).max(max_param(len)),
        TyKind::FnDef(_) | TyKind::FnPtr(_) => ty.children().map(max_param).max().unwrap_or(0),
        _ => 0,
    }
}

/// Reports names defined twice by blocks for overlapping instances of a
/// generic struct, `impl<T> S<T>` and `impl S<i32>`; the item tree
/// checked other types, and blocks for one generic struct that cannot
/// overlap (`impl S<u8>` and `impl S<i32>`) may share names. Only pairs
/// whose later block is from `from` on are checked.
fn duplicate_assoc_items(
    items: &ItemTree,
    sigs: &Signatures,
    diagnostics: &mut Vec<Diagnostic>,
    from: usize,
) {
    let generic: Vec<(ImplId, &Impl)> = items
        .impls
        .iter()
        .filter(|(_, imp)| match imp.target {
            Some(ImplTarget::Struct(s)) => !items.structs[s].generics.is_empty(),
            _ => false,
        })
        .collect();
    let names = |imp: &Impl| -> Vec<(Symbol, TextRange)> {
        let functions = imp.functions.iter().map(|&f| {
            let f = &items.functions[f];
            (f.name, f.def_range)
        });
        let consts = imp.consts.iter().filter_map(|&c| {
            let c = &items.consts[c];
            Some((c.name?, c.def_range))
        });
        functions.chain(consts).collect()
    };
    for (i, &(a, first)) in generic.iter().enumerate() {
        for &(b, second) in &generic[i + 1..] {
            if b.index() < from
                || first.target != second.target
                || !overlap(sigs.impls[a], sigs.impls[b])
            {
                continue;
            }
            let later = names(second);
            // rustc reports the earlier definition.
            for (name, range) in names(first) {
                if later.iter().any(|&(n, _)| n == name) {
                    diagnostics.push(Diagnostic {
                        file: first.file,
                        range,
                        message: format!("duplicate definitions with name `{name}`"),
                    });
                }
            }
        }
    }
}

/// Whether two `impl` blocks' types, each in terms of its own
/// parameters, may be the same type.
pub(crate) fn overlap(a: Ty, b: Ty) -> bool {
    match (a.kind(), b.kind()) {
        (TyKind::Param(_) | TyKind::Error, _) | (_, TyKind::Param(_) | TyKind::Error) => true,
        (TyKind::Adt(x, xs), TyKind::Adt(y, ys)) => {
            x == y && xs.iter().zip(ys).all(|(&a, &b)| overlap(a, b))
        }
        (TyKind::Tuple(xs), TyKind::Tuple(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys).all(|(&a, &b)| overlap(a, b))
        }
        (TyKind::Ref(m, x), TyKind::Ref(n, y)) | (TyKind::RawPtr(m, x), TyKind::RawPtr(n, y)) => {
            m == n && overlap(x, y)
        }
        (TyKind::Array(x, m), TyKind::Array(y, n)) => overlap(m, n) && overlap(x, y),
        (TyKind::Slice(x), TyKind::Slice(y)) => overlap(x, y),
        (TyKind::FnPtr(x), TyKind::FnPtr(y)) => {
            x.is_unsafe == y.is_unsafe
                && x.is_c == y.is_c
                && x.inputs_output.len() == y.inputs_output.len()
                && (x.inputs_output.iter().zip(y.inputs_output)).all(|(&a, &b)| overlap(a, b))
        }
        (TyKind::FnDef(x), TyKind::FnDef(y)) => {
            x.function == y.function && x.args.iter().zip(y.args).all(|(&a, &b)| overlap(a, b))
        }
        _ => a == b,
    }
}

/// Whether `ty` mentions generic parameter `index`.
pub(crate) fn mentions_param(ty: Ty, index: u32) -> bool {
    if !ty.has_params() {
        return false;
    }
    match ty.kind() {
        TyKind::Param(param) => param.index == index,
        TyKind::RawPtr(_, t) | TyKind::Ref(_, t) | TyKind::Slice(t) => mentions_param(t, index),
        TyKind::Array(t, len) => mentions_param(t, index) || mentions_param(len, index),
        TyKind::Tuple(list) | TyKind::Adt(_, list) | TyKind::Closure(_, list) => {
            list.iter().any(|&t| mentions_param(t, index))
        }
        TyKind::FnDef(_) | TyKind::FnPtr(_) => ty.children().any(|t| mentions_param(t, index)),
        TyKind::Opaque(opaque) => opaque.args.iter().any(|&t| mentions_param(t, index)),
        _ => false,
    }
}

/// Whether `ty` uses generic parameter `index`, through associated types
/// (`T::Out`) too, as a struct's fields may.
fn uses_param(ty: Ty, index: u32) -> bool {
    match ty.kind() {
        TyKind::Alias(alias) => alias
            .args
            .iter()
            .chain(alias.family_args)
            .any(|&t| uses_param(t, index)),
        TyKind::Opaque(opaque) => opaque.args.iter().any(|&t| uses_param(t, index)),
        TyKind::RawPtr(_, t) | TyKind::Ref(_, t) | TyKind::Slice(t) => uses_param(t, index),
        TyKind::Array(t, len) => uses_param(t, index) || mentions_param(len, index),
        TyKind::Tuple(list) | TyKind::Adt(_, list) | TyKind::Closure(_, list) => {
            list.iter().any(|&t| uses_param(t, index))
        }
        TyKind::FnPtr(_) => ty.children().any(|t| uses_param(t, index)),
        _ => mentions_param(ty, index),
    }
}

/// The parameters of `generics` as types, numbered from `first`.
/// The bounds of `function`'s opaque types, resolved in `scope`, its
/// signature's: each in terms of the function's generic parameters and,
/// for the opaque type itself, one after them.
fn opaque_sigs(
    items: &ItemTree,
    function: &flux_hir::Function,
    scope: TypeScope<'_>,
    errors: &mut Vec<(TextRange, String)>,
) -> Box<[OpaqueSig]> {
    let module = function.loc.module;
    let count = scope.generics.len() as u32;
    let mut out = Vec::new();
    for decl in function.opaques.iter() {
        let this = Ty::param(count, decl.name);
        let mut predicates = Vec::new();
        let mut projections = Vec::new();
        for b in decl.bounds.iter() {
            if let Some((trait_ref, bindings)) =
                traits::resolve_bound(items, module, b, this, scope, errors)
            {
                predicates.push(Predicate {
                    self_ty: this,
                    trait_ref,
                });
                projections.extend(bindings);
            }
        }
        // `use<..>` must list every type parameter in scope, as rustc
        // requires so far.
        if let Some(captures) = &decl.captures {
            let listed = |name: Symbol| {
                captures
                    .iter()
                    .any(|c| matches!(c, flux_hir::UseCapture::Param(n, _) if *n == name))
            };
            let generics = items
                .owner_generics(function.owner)
                .into_iter()
                .chain([&function.generics]);
            if generics
                .flat_map(|g| g.params.iter())
                .any(|p| p.synthetic || !listed(p.name))
            {
                errors.push((
                    decl.range,
                    "`impl Trait` must mention all type parameters in scope in `use<...>`"
                        .to_owned(),
                ));
            }
            for c in captures.iter() {
                if let flux_hir::UseCapture::Param(name, range) = *c
                    && scope.param(name).is_none()
                    && name != kw::SelfUpper
                {
                    errors.push((range, format!("cannot find type `{name}` in this scope")));
                }
            }
        }
        out.push(OpaqueSig {
            predicates: predicates.into(),
            projections: projections.into(),
        });
    }
    out.into()
}

/// The generic parameters called `names`, in order.
fn params_of_names(names: &[Symbol]) -> Vec<Ty> {
    (names.iter().enumerate())
        .map(|(i, &name)| Ty::param(i as u32, name))
        .collect()
}

pub fn params_of(generics: &Generics, first: usize) -> Vec<Ty> {
    generics
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| Ty::param((first + i) as u32, p.name))
        .collect()
}

/// Reports what is wrong with type alias `id` itself: errors in its
/// type, a cycle of aliases (once, at its first alias), parameters it
/// does not use, and traits its (unenforced) bounds name that do not
/// resolve.
fn check_alias(
    items: &ItemTree,
    id: flux_hir::TypeAliasId,
    alias: &flux_hir::TypeAlias,
    lens: Option<&ArrayLens<'_>>,
    sigs: &Signatures,
    reported: &mut Vec<Diagnostic>,
) {
    let mut push = |range, message: String| {
        reported.push(Diagnostic {
            file: alias.loc.file,
            range,
            message,
        });
    };
    for p in alias.generics.predicates.iter() {
        if let (_, Some((range, message))) = items.resolve_trait(alias.loc.module, &p.bound) {
            push(range, message);
        }
    }
    if alias_cycle(items, id) {
        let earlier = items
            .type_aliases
            .iter()
            .take(id.index())
            .any(|(other, _)| {
                reaches_alias(items, other, id, &mut Vec::new())
                    && reaches_alias(items, id, other, &mut Vec::new())
            });
        if !earlier {
            push(
                alias.ty_range,
                format!("cycle detected when expanding type alias `{}`", alias.name),
            );
        }
        return;
    }
    let mut errors = Vec::new();
    let names = generic_names(items, None, &alias.generics);
    let consts = generic_consts(items, None, &alias.generics);
    let unsized_flags = unsized_params(items, None, &alias.generics);
    let scope = TypeScope {
        generics: &names,
        consts: &consts,
        unsized_params: &unsized_flags,
        signature_of: Some("type aliases"),
        lens,
        defaults: Some(&sigs.trait_defaults),
        struct_fields: Some(&sigs.structs),
        ..TypeScope::default()
    };
    let bounds =
        traits::resolve_predicates(items, alias.loc.module, &alias.generics, scope, &mut errors);
    check_type_defaults(
        items,
        &alias.generics,
        alias.loc.module,
        TypeScope {
            bounds: &bounds.predicates,
            ..scope
        },
        &mut errors,
    );
    let ty = expand_alias(items, id, lens, Some(&sigs.trait_defaults), &mut errors);
    for (range, message) in errors {
        push(range, message);
    }
    if ty == Ty::ERROR {
        return;
    }
    for (i, param) in alias.generics.params.iter().enumerate() {
        if !param.is_const() && !uses_param(ty, i as u32) {
            push(
                param.range,
                format!("type parameter `{}` is never used", param.name),
            );
        }
    }
}

/// The innermost type preventing a shared static from being `Sync`.
/// Use the same rules as bounds, including explicit unsafe impls.
fn not_sync(items: &ItemTree, sigs: &Signatures, ty: Ty, seen: &mut Vec<StructId>) -> Option<Ty> {
    let sync = *items.lang_traits.get(&Symbol::intern("sync"))?;
    let goal = traits::Predicate {
        self_ty: ty,
        trait_ref: traits::TraitRef {
            trait_: sync,
            name: items.traits[sync].name,
            args: Ty::list(&[]),
        },
    };
    if traits::holds(items, sigs, &[], goal) {
        return None;
    }
    if let TyKind::Adt(adt, _) = ty.kind() {
        if seen.contains(&adt.strukt) {
            return Some(ty);
        }
        seen.push(adt.strukt);
    }
    if let Some(nested) = traits::auto_trait_requirements(items, sigs, goal) {
        for p in nested {
            if let Some(cause) = not_sync(items, sigs, p.self_ty, seen) {
                return Some(cause);
            }
        }
    }
    Some(ty)
}

/// Whether a value of `ty`, which is `strukt` itself at the top, contains
/// another `strukt` by value; `seen` guards against cycles that do not
/// involve `strukt`.
fn contains(sigs: &Signatures, ty: Ty, strukt: StructId, seen: &mut Vec<StructId>) -> bool {
    if let TyKind::Adt(adt, _) = ty.kind() {
        if adt.strukt == strukt && !seen.is_empty() {
            return true;
        }
        if seen.contains(&adt.strukt) {
            return false;
        }
        seen.push(adt.strukt);
    }
    let fields = sigs.contents(ty).unwrap_or(&[]);
    fields
        .iter()
        .any(|&field| contains(sigs, field, strukt, seen))
}

/// The type of enum `strukt`'s discriminant: its `repr`'s, or `isize`.
pub fn discriminant_ty(items: &ItemTree, strukt: StructId) -> IntTy {
    items.structs[strukt]
        .repr
        .and_then(IntTy::from_name)
        .unwrap_or(IntTy::Isize)
}

/// The fields of a struct, or of every variant of an enum in turn: what
/// [`Signatures::structs`] has the types of.
pub fn all_fields(s: &flux_hir::Struct) -> impl Iterator<Item = &flux_hir::Field> {
    s.fields
        .iter()
        .chain(s.variants.iter().flatten().flat_map(|v| v.fields.iter()))
}

/// The type of `impl` block `imp` (`S<T>` in `impl<T> S<T>`), in terms of
/// its parameters; an error if its path did not resolve (reported).
fn impl_type(
    items: &ItemTree,
    imp: &Impl,
    lens: Option<&ArrayLens<'_>>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Ty {
    let generics: Vec<Symbol> = imp.generics.params.iter().map(|p| p.name).collect();
    let consts: Vec<Option<Ty>> = imp.generics.params.iter().map(param_const_ty).collect();
    let mut errors = Vec::new();
    check_const_params(items, imp.module, &imp.generics, &generics, &mut errors);
    diagnostics.extend(errors.into_iter().map(|(range, message)| Diagnostic {
        file: imp.file,
        range,
        message,
    }));
    let scope = TypeScope {
        self_ty: None,
        generics: &generics,
        consts: &consts,
        signature_of: Some("implementations"),
        lens,
        ..TypeScope::default()
    };
    // The impl header may project through its own generic bounds, as in
    // `impl<A: Array> Trait for [A::Item]`. The later signature pass checks
    // these predicates completely once trait defaults and supertraits exist.
    let preliminary =
        traits::resolve_predicates(items, imp.module, &imp.generics, scope, &mut Vec::new());
    let scope = TypeScope {
        bounds: &preliminary.predicates,
        ..scope
    };
    let mut errors = Vec::new();
    if imp.trait_ref.is_some() {
        // Only the diagnostics added here: looking through all of them
        // for each `impl` block would be quadratic.
        let start = diagnostics.len();
        let ty = resolve_or_report(
            items,
            imp.module,
            &imp.self_ty,
            scope,
            imp.self_range,
            diagnostics,
        );
        for d in &mut diagnostics[start..] {
            d.file = imp.file;
        }
        return ty;
    }
    let through_alias = |path: &Path| {
        matches!(
            items.resolve(imp.module, path, Ns::Types),
            Ok(Resolved::Def(Def::TypeAlias(_)))
                | Err(ResolveError::Private {
                    resolved: Resolved::Def(Def::TypeAlias(_)),
                    ..
                })
        )
    };
    let ty = match (imp.target, &imp.self_ty) {
        // `impl A` for `type A = S<u8>;`: the alias's type.
        (Some(ImplTarget::Struct(_)), TypeRef::Path(path)) if through_alias(path) => resolve_type(
            items,
            imp.module,
            &imp.self_ty,
            scope,
            imp.self_range,
            &mut errors,
        ),
        (Some(ImplTarget::Struct(strukt)), TypeRef::Path(path)) => struct_type(
            items,
            imp.module,
            strukt,
            path,
            scope,
            imp.self_range,
            &mut errors,
        ),
        (
            Some(ImplTarget::Primitive(_)),
            TypeRef::Slice(_) | TypeRef::Array { .. } | TypeRef::Ptr { .. },
        ) => resolve_type(
            items,
            imp.module,
            &imp.self_ty,
            scope,
            imp.self_range,
            &mut errors,
        ),
        (Some(ImplTarget::Primitive(name)), _) => primitive(name).unwrap_or(Ty::ERROR),
        (Some(ImplTarget::Object(_)), TypeRef::Dyn(_)) => resolve_type(
            items,
            imp.module,
            &imp.self_ty,
            scope,
            imp.self_range,
            &mut errors,
        ),
        _ => Ty::ERROR,
    };
    diagnostics.extend(errors.into_iter().map(|(at, message)| Diagnostic {
        file: imp.file,
        range: at,
        message,
    }));
    ty
}

/// Checks that a program's `main` exists and has the right shape.
pub fn check_main(items: &ItemTree, sigs: &Signatures) -> Option<Diagnostic> {
    let krate = items.main_crate();
    let root = items.module_of(krate);
    let data = &items.crates[krate.0 as usize];
    if data.no_std && !data.externs.contains(&sym::std) {
        return Some(Diagnostic {
            file: items.modules[root].file,
            range: TextRange::empty(0),
            message: "`no_std` executables without `extern crate std` are not supported yet".into(),
        });
    }
    let main = items.functions.iter().find(|(_, f)| {
        f.name == sym::main && !f.is_extern && f.owner.is_none() && f.loc.module == root
    });
    let Some((id, function)) = main else {
        let name = items.crates[krate.0 as usize].name;
        let mut d = Diagnostic::new(
            TextRange::empty(0),
            format!("`main` function not found in crate `{name}`"),
        );
        d.file = items.modules[root].file;
        return Some(d);
    };
    let sig = &sigs[id];
    let message = if !function.generics.is_empty() {
        let mut d = Diagnostic::new(
            function.generics.range,
            "`main` function is not allowed to have generic parameters",
        );
        d.file = function.loc.file;
        return Some(d);
    } else if function.c_abi {
        "`main` function must have the Rust ABI".to_owned()
    } else if function.target_features.is_some() {
        "`main` function is not allowed to have `target_feature`".to_owned()
    } else if function.is_unsafe {
        "`main` function is not allowed to be `unsafe`".to_owned()
    } else if !sig.params.is_empty() {
        "`main` function cannot take parameters".to_owned()
    } else if ![Ty::UNIT, Ty::NEVER, Ty::ERROR].contains(&sig.ret) {
        format!("`main` has invalid return type `{}`", sig.ret)
    } else {
        return None;
    };
    let mut d = Diagnostic::new(function.name_range, message);
    d.file = function.loc.file;
    Some(d)
}

/// A file with every body inferred.
#[derive(Debug)]
pub struct TypedFile {
    pub sigs: Signatures,
    pub bodies: ArenaMap<Function, InferenceResult>,
    pub consts: ArenaMap<Const, InferenceResult>,
    /// From signatures and every body, in source order.
    pub diagnostics: Vec<Diagnostic>,
}

/// Whether a trait path in a bound or an `impl` block did not resolve:
/// rustc stops before type checking then.
fn unresolved_trait(items: &ItemTree) -> bool {
    let unresolved =
        |module, bound: &flux_hir::Bound| items.resolve_trait(module, bound).0.is_none();
    let in_generics = |module, generics: &Generics| {
        generics
            .predicates
            .iter()
            .any(|p| unresolved(module, &p.bound))
    };
    items.impls.values().any(|imp| {
        (imp.trait_ref.is_some() && imp.trait_.is_none()) || in_generics(imp.module, &imp.generics)
    }) || items.traits.values().any(|t| {
        in_generics(t.loc.module, &t.generics)
            || t.supertraits.iter().any(|b| unresolved(t.loc.module, b))
            || (t.types.iter()).any(|ty| ty.bounds.iter().any(|b| unresolved(t.loc.module, b)))
    }) || items
        .functions
        .values()
        .any(|f| in_generics(f.loc.module, &f.generics))
}

/// Infers every body of the program, and those of `core` it may call.
/// Array lengths may name only constants whose values are integer
/// literals: evaluating others needs MIR, which
/// [`check_file_with`]'s caller provides.
pub fn check_file(hir: &Hir) -> TypedFile {
    check(hir, false, &literal_lens)
}

/// Like [`check_file`], with every body of `core` too: for tests of
/// `core` itself.
pub fn check_file_all(hir: &Hir) -> TypedFile {
    check(hir, true, &literal_lens)
}

/// Like [`check_file`], with `eval` evaluating the constants array
/// lengths name.
pub fn check_file_with(hir: &Hir, eval: LenEvaluator<'_>) -> TypedFile {
    check(hir, false, eval)
}

/// The values of constants whose values are integer literals: what
/// array lengths can name without constant evaluation.
fn literal_lens(
    hir: &Hir,
    _: &Signatures,
    wanted: &[ConstId],
) -> (Vec<(ConstId, Option<u128>)>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let values = wanted
        .iter()
        .map(|&konst| {
            let value =
                hir.const_body(konst)
                    .and_then(|(body, source_map)| match &body.exprs[body.root] {
                        flux_hir::Expr::Literal(flux_hir::Literal::Int { value, .. }) => {
                            Some(*value)
                        }
                        flux_hir::Expr::Literal(flux_hir::Literal::Bool(b)) => Some(u128::from(*b)),
                        _ => {
                            let mut d = Diagnostic::new(
                                source_map.exprs.get(body.root).copied().unwrap_or_default(),
                                "evaluating this constant needs MIR",
                            );
                            d.file = hir.items.consts[konst].loc.file;
                            diagnostics.push(d);
                            None
                        }
                    });
            (konst, value)
        })
        .collect();
    (values, diagnostics)
}

/// The values of `core`'s constants that its array lengths may name:
/// those whose values are literals, which need no evaluation (`static
/// mut BUF: [u8; CAPACITY]`).
fn core_lens_values() -> LenValues {
    let core = Core::get();
    literal_values(
        (core.items.consts.iter()).filter_map(|(id, _)| core.const_body(id).map(|b| (id, b))),
    )
}

/// The values of those of `consts` whose values are integer or `bool`
/// literals.
fn literal_values<'b>(
    consts: impl Iterator<Item = (ConstId, &'b (flux_hir::Body, flux_hir::BodySourceMap))>,
) -> LenValues {
    consts
        .filter_map(|(id, (body, _))| match &body.exprs[body.root] {
            flux_hir::Expr::Literal(flux_hir::Literal::Int { value, .. }) => {
                Some((id, Some(*value)))
            }
            flux_hir::Expr::Literal(flux_hir::Literal::Bool(b)) => Some((id, Some(u128::from(*b)))),
            _ => None,
        })
        .collect()
}

/// Resolves the signatures of `items`, evaluating with `eval` the
/// constants their array lengths name, if any: then they are resolved
/// twice, the first time to type-check the constants.
fn lens_signatures(hir: &Hir, all: bool, eval: LenEvaluator<'_>) -> (Signatures, Vec<Diagnostic>) {
    let items = &hir.items;
    // Most constants lengths name are literals, known without
    // evaluation; the others need the signatures resolved first.
    let mut known = literal_values(hir.const_bodies());
    let mut arg_tys = ArgTys::default();
    let lens = ArrayLens::new(&known, &arg_tys);
    let (mut sigs, diagnostics) = signatures_checked(items, all, Some(&lens));
    let missing = lens.missing.into_inner();
    let new_tys = lens.new_arg_tys.into_inner();
    if missing.is_empty() && new_tys.is_empty() {
        sigs.array_lens = known;
        return (sigs, diagnostics);
    }
    arg_tys.extend(new_tys);
    for (&konst, &ty) in &arg_tys {
        sigs.consts.insert(konst, ty);
    }
    sigs.arg_tys = arg_tys.clone();
    sigs.array_lens = known.clone();
    let (values, mut failures) = eval(hir, &sigs, &missing);
    known.extend(values);
    let lens = ArrayLens::new(&known, &arg_tys);
    let (mut sigs, mut diagnostics) = signatures_checked(items, all, Some(&lens));
    sigs.array_lens = known;
    sigs.arg_tys = arg_tys;
    diagnostics.append(&mut failures);
    (sigs, diagnostics)
}

/// The embedded core, alloc and std destructors, which implicit drop glue
/// may call even when no explicit call reaches their bodies.
pub fn core_drop_fns(items: &ItemTree) -> Vec<FunctionId> {
    let Some(&t) = items.lang_traits.get(&sym::drop) else {
        return Vec::new();
    };
    let impls = items.trait_impls.get(&t).into_iter().flatten();
    impls
        .flat_map(|&imp| items.impls[imp].functions.iter().copied())
        // std shares the embedded cache but cannot define trusted lang items.
        // Its destructors are still roots, just like core and alloc's.
        .filter(|f| f.index() < items.core.functions as usize)
        .collect()
}

/// `core`'s functions code generation calls without MIR naming them:
/// those a failed check panics through, and what the entry point calls
/// after `main`, by their `#[lang]` names.
pub const RUNTIME_LANG_FNS: &[&str] = &[
    "panic_bounds_check",
    "panic_const_div_by_zero",
    "panic_const_rem_by_zero",
    "panic_const_div_overflow",
    "panic_const_rem_overflow",
    "flush_stdout",
];

/// `core`'s functions every program may need whether or not its code
/// calls them: its `Drop` impls, which drop glue may call, and
/// [`RUNTIME_LANG_FNS`].
pub fn core_roots(items: &ItemTree) -> Vec<FunctionId> {
    let mut roots = core_drop_fns(items);
    roots.extend(
        RUNTIME_LANG_FNS
            .iter()
            .filter_map(|lang| items.lang_fns.get(&Symbol::intern(lang)).copied()),
    );
    roots
}

/// `Deref::deref`, or `DerefMut::deref_mut` for `Mut`: what an
/// overloaded dereference calls.
pub fn deref_fn(items: &ItemTree, mutability: Mutability) -> Option<FunctionId> {
    let (lang, name) = match mutability {
        Mutability::Not => ("deref", "deref"),
        Mutability::Mut => ("deref_mut", "deref_mut"),
    };
    let t = *items.lang_traits.get(&Symbol::intern(lang))?;
    items.traits[t].function(items, Symbol::intern(name))
}

/// The functions a call of `function` may run: itself, and for a
/// trait's method, its definition in each `impl` block of the trait.
pub fn call_targets(items: &ItemTree, function: FunctionId) -> Vec<FunctionId> {
    let mut out = vec![function];
    if let Some(Owner::Trait(t)) = items.functions[function].owner {
        let name = items.functions[function].name;
        let impls = items.trait_impls.get(&t).into_iter().flatten();
        out.extend(impls.flat_map(|&imp| {
            items.impls[imp]
                .functions
                .iter()
                .copied()
                .filter(move |&f| items.functions[f].name == name)
        }));
    }
    out
}

fn check(hir: &Hir, all: bool, eval: LenEvaluator<'_>) -> TypedFile {
    let (mut sigs, mut diagnostics) = lens_signatures(hir, all, eval);
    let mut bodies = ArenaMap::new();
    if sigs.cyclic || unresolved_trait(&hir.items) {
        Diagnostic::sort(&mut diagnostics);
        return TypedFile {
            sigs,
            bodies,
            consts: ArenaMap::new(),
            diagnostics,
        };
    }
    let items = &hir.items;
    let mut queue: Vec<FunctionId> = Vec::new();
    // The functions whose targets are queued already: a trait method
    // called from every body would queue all its blocks' for each.
    let mut expanded = rustc_hash::FxHashSet::default();
    let mut reached = |result: &InferenceResult, queue: &mut Vec<FunctionId>| {
        let called = result.calls.iter().map(|(_, i)| i.function);
        let methods = result.methods.iter().map(|(_, m)| m.function);
        let operators = result.overloads.iter().map(|(_, i)| i.function);
        let conversions = result.try_conversions.iter().map(|(_, i)| i.function);
        // Functions named as values (`.map(Some)`), which their
        // `Fn`-family impls call.
        let values = result
            .expr_types
            .iter()
            .filter_map(|(_, ty)| match ty.kind() {
                TyKind::FnDef(def) => Some(def.function),
                _ => None,
            });
        // A value made a trait object may have any method of its
        // trait and supertraits called, through the vtable, and be
        // dropped in place through it.
        let traits: Vec<flux_hir::TraitId> = result
            .unsizes
            .iter()
            .filter_map(|(_, &ty)| objects::unsize_trait(ty))
            .collect();
        let drop = (!traits.is_empty())
            .then(|| objects::drop_in_place(items))
            .flatten();
        let objects: Vec<FunctionId> = traits
            .into_iter()
            .flat_map(|t| objects::supertraits_of(items, t))
            .flat_map(|t| items.traits[t].functions.iter().copied())
            .chain(drop)
            .collect();
        // `Deref::deref` and `DerefMut::deref_mut`, of overloaded
        // dereferences.
        let derefs = result
            .overloaded_derefs
            .iter()
            .chain(result.coercion_derefs.iter())
            .flat_map(|(_, steps)| steps.iter())
            .filter_map(|step| deref_fn(items, step.mutability));
        for f in called
            .chain(methods)
            .chain(operators)
            .chain(conversions)
            .chain(values)
            .chain(objects)
            .chain(derefs)
        {
            if expanded.insert(f) {
                queue.extend(call_targets(items, f));
            }
        }
    };
    let mut consts = ArenaMap::new();
    for (id, (body, source_map)) in hir.const_bodies() {
        let result = infer_const(items, &sigs, id, body, source_map);
        reached(&result, &mut queue);
        consts.insert(id, result);
    }
    // `core`'s `Drop` impls, which drop glue may call, and the
    // functions code generation calls, then the program's bodies, then
    // the `core` bodies they reach.
    queue.extend(core_roots(items));
    queue.extend(
        hir.bodies
            .iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
            .into_iter()
            .rev(),
    );
    if all {
        queue.extend(items.functions.iter().map(|(id, _)| id));
    }
    while let Some(id) = queue.pop() {
        if bodies.get(id).is_some() {
            continue;
        }
        let Some((body, source_map)) = hir.body(id) else {
            continue;
        };
        let result = infer_body(items, &sigs, id, body, source_map);
        reached(&result, &mut queue);
        bodies.insert(id, result);
    }
    // Array lengths in bodies that name constants not evaluated yet:
    // evaluated now, and those bodies inferred again; so are the
    // constants of const generic arguments whose types they found.
    let mut missing: Vec<ConstId> = Vec::new();
    let mut new_tys: Vec<ConstId> = Vec::new();
    let results = consts
        .iter()
        .map(|(_, r)| r)
        .chain(bodies.iter().map(|(_, r)| r));
    for result in results {
        for &c in &result.missing_lens {
            if !missing.contains(&c) {
                missing.push(c);
            }
        }
        for &(c, ty) in &result.new_arg_tys {
            if sigs.arg_tys.insert(c, ty).is_none() {
                sigs.consts.insert(c, ty);
                new_tys.push(c);
            }
        }
    }
    if !missing.is_empty() || !new_tys.is_empty() {
        let (values, failures) = eval(hir, &sigs, &missing);
        sigs.array_lens.extend(values);
        diagnostics.extend(failures);
        let redo: Vec<ConstId> = consts
            .iter()
            .filter(|(id, r)| !r.missing_lens.is_empty() || new_tys.contains(id))
            .map(|(id, _)| id)
            .collect();
        for id in redo {
            let (body, source_map) = hir.const_body(id).expect("inferred above");
            consts.insert(id, infer_const(items, &sigs, id, body, source_map));
        }
        let redo: Vec<FunctionId> = bodies
            .iter()
            .filter(|(_, r)| !r.missing_lens.is_empty() || !r.new_arg_tys.is_empty())
            .map(|(id, _)| id)
            .collect();
        for id in redo {
            let (body, source_map) = hir.body(id).expect("inferred above");
            bodies.insert(id, infer_body(items, &sigs, id, body, source_map));
        }
    }
    // What code generation sees of opaque types.
    for (id, result) in bodies.iter() {
        for (i, &ty) in result.opaques.iter().enumerate() {
            sigs.hidden.insert((id, i as u32), ty);
        }
    }
    for (id, result) in consts.iter() {
        let file = items.consts[id].loc.file;
        diagnostics.extend(
            result
                .diagnostics
                .iter()
                .map(|d| Diagnostic { file, ..d.clone() }),
        );
    }
    for (id, result) in bodies.iter() {
        let file = items.functions[id].loc.file;
        diagnostics.extend(
            result
                .diagnostics
                .iter()
                .map(|d| Diagnostic { file, ..d.clone() }),
        );
    }
    for diagnostic in &mut diagnostics {
        items.resolve_diagnostic(diagnostic);
    }
    Diagnostic::sort(&mut diagnostics);
    TypedFile {
        sigs,
        bodies,
        consts,
        diagnostics,
    }
}

/// A structural constant's scalar leaves and aggregate fields, in order.
/// Pattern checking consumes values rather than assuming a constant is a
/// wildcard; a newtype constant covers only its actual field values.
#[derive(Debug, Clone)]
pub enum PatternConst {
    Scalar(i128),
    Aggregate(Vec<PatternConst>),
    Variant {
        index: u32,
        fields: Vec<PatternConst>,
    },
}

/// Whether a constant of `ty` can be matched structurally. ADTs require
/// derived PartialEq; a handwritten equality method cannot define patterns.
/// Enum payloads are checked against the evaluated active variant; references
/// and pointer constants remain outside this subset.
pub fn structural_pattern_type(items: &ItemTree, sigs: &Signatures, ty: Ty) -> bool {
    fn check(items: &ItemTree, sigs: &Signatures, ty: Ty, depth: u32) -> bool {
        if depth >= 128 {
            return false;
        }
        match ty.kind() {
            TyKind::Int(_) | TyKind::Bool | TyKind::Char | TyKind::Error => true,
            TyKind::Tuple(fields) => fields.iter().all(|&t| check(items, sigs, t, depth + 1)),
            TyKind::Array(elem, len) => {
                len.as_usize().is_some() && check(items, sigs, elem, depth + 1)
            }
            TyKind::Adt(adt, _) => {
                !items.structs[adt.strukt].is_union
                    && items
                        .functions
                        .values()
                        .any(|f| f.derived == Some((adt.strukt, flux_hir::Derive::PartialEq)))
                    && (sigs.is_enum(adt.strukt)
                        || sigs.fields(ty).is_some_and(|fields| {
                            fields.iter().all(|&t| check(items, sigs, t, depth + 1))
                        }))
            }
            _ => false,
        }
    }
    check(items, sigs, ty, 0)
}

/// The checks of the patterns of the bodies whose patterns name
/// constants ([`InferenceResult::deferred_pats`]), given `value`, the
/// scalar leaves and aggregate fields of a structural constant, or
/// `None` if it has no value (reported). Bodies that name one without a
/// value are not checked. Also returns the functions with errors, which
/// rustc does not borrow-check.
pub fn check_deferred_patterns(
    hir: &Hir,
    typed: &TypedFile,
    value: &mut dyn FnMut(ConstInstance) -> Option<PatternConst>,
) -> (Vec<Diagnostic>, Vec<FunctionId>) {
    let functions = typed.bodies.iter().map(|(id, result)| {
        let file = hir.items.functions[id].loc.file;
        (
            Some(id),
            hir.body(id),
            file,
            hir.items.functions[id].loc.module,
            result,
        )
    });
    let consts = typed.consts.iter().map(|(id, result)| {
        let file = hir.items.consts[id].loc.file;
        (
            None,
            hir.const_body(id),
            file,
            hir.items.consts[id].loc.module,
            result,
        )
    });
    let mut diagnostics = Vec::new();
    let mut failed = Vec::new();
    'bodies: for (function, body, file, module, result) in functions.chain(consts) {
        let Some((body, source_map)) = body else {
            continue;
        };
        if !result.deferred_pats {
            continue;
        }
        let mut values = ArenaMap::new();
        for data in body.pats.values() {
            let ends = match *data {
                flux_hir::Pat::Expr(expr) => [Some(expr), None],
                flux_hir::Pat::Range { start, end, .. } => [start, end],
                _ => continue,
            };
            for expr in ends.into_iter().flatten() {
                let instance = match (result.consts.get(expr), &body.exprs[expr]) {
                    (Some(&instance), _) => instance,
                    (None, &flux_hir::Expr::Path(flux_hir::Res::Const(konst))) => {
                        ConstInstance { konst, args: &[] }
                    }
                    _ => continue,
                };
                match value(instance) {
                    Some(v) => {
                        values.insert(expr, v);
                    }
                    None => continue 'bodies,
                }
            }
        }
        let found = pats::check_body(
            &hir.items,
            module,
            &typed.sigs,
            body,
            source_map,
            &result.pat_types,
            &result.pat_derefs,
            &result.expr_types,
            &values,
            &[],
        );
        if !found.is_empty() {
            failed.extend(function);
        }
        diagnostics.extend(found.into_iter().map(|d| Diagnostic { file, ..d }));
    }
    (diagnostics, failed)
}

impl TypedFile {
    /// Each function's signature and the type of each of its bindings,
    /// then the diagnostics. Used by snapshot tests.
    pub fn debug_dump(&self, hir: &Hir) -> String {
        let mut out = String::new();
        let items = &hir.items;
        for (id, s) in items.structs.iter() {
            if !items.in_main_crate(s.loc.module) {
                continue;
            }
            if let Some(variants) = &s.variants {
                let starts = &self.sigs.variant_starts[id];
                let all = self.sigs.structs[id];
                let shown: Vec<String> = variants
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let types = &all[starts[i] as usize..starts[i + 1] as usize];
                        let fields: Vec<String> = v
                            .fields
                            .iter()
                            .zip(types)
                            .map(|(f, ty)| format!("{}: {ty}", f.name))
                            .collect();
                        match fields.as_slice() {
                            [] => v.name.to_string(),
                            _ => format!("{} {{ {} }}", v.name, fields.join(", ")),
                        }
                    })
                    .collect();
                writeln!(
                    out,
                    "enum {}{} {{ {} }}",
                    s.name,
                    s.generics,
                    shown.join(", ")
                )
                .unwrap();
                continue;
            }
            let fields: Vec<String> = s
                .fields
                .iter()
                .zip(self.sigs.structs[id])
                .map(|(f, ty)| format!("{}: {ty}", f.name))
                .collect();
            match fields.as_slice() {
                [] => writeln!(out, "struct {}{} {{}}", s.name, s.generics),
                _ => writeln!(
                    out,
                    "struct {}{} {{ {} }}",
                    s.name,
                    s.generics,
                    fields.join(", ")
                ),
            }
            .unwrap();
        }
        for (id, function) in items.functions.iter() {
            if !hir.is_program(id) || function.ctor.is_some() {
                continue;
            }
            let sig = &self.sigs[id];
            let params: Vec<String> = sig.params.iter().map(Ty::to_string).collect();
            writeln!(
                out,
                "fn {}{}({}) -> {}",
                function.name,
                function.generics,
                params.join(", "),
                sig.ret
            )
            .unwrap();
            if let (Some((body, _)), Some(result)) = (hir.bodies.get(id), self.bodies.get(id)) {
                for (binding, data) in body.bindings.iter() {
                    let ty = result
                        .binding_types
                        .get(binding)
                        .copied()
                        .unwrap_or(Ty::ERROR);
                    writeln!(out, "    {}@{}: {ty}", data.name, binding.index()).unwrap();
                }
            }
        }
        let lens = ArrayLens::new(&self.sigs.array_lens, &self.sigs.arg_tys);
        for (id, alias) in items.type_aliases.iter() {
            if items.in_main_crate(alias.loc.module) {
                let ty = expand_alias(
                    items,
                    id,
                    Some(&lens),
                    Some(&self.sigs.trait_defaults),
                    &mut Vec::new(),
                );
                writeln!(out, "type {}{} = {ty}", alias.name, alias.generics).unwrap();
            }
        }
        for (id, konst) in items.consts.iter() {
            if items.in_main_crate(konst.loc.module) {
                let name = items.const_name(id);
                writeln!(out, "const {name}: {}", self.sigs.consts[id]).unwrap();
            }
        }
        hir.write_diagnostics(&mut out, &self.diagnostics);
        out
    }
}

/// `f` as written in messages.
fn function_name(items: &ItemTree, f: FunctionId) -> Symbol {
    items.functions[f].name
}

/// A deliberately region-free, sized subset. Check every substituted field,
/// including enum variants and nested unions, rather than treating an ADT's
/// Copy impl as proof that its representation is supported. Reject recursive
/// generic arguments conservatively, even when they occur only in phantom data.
fn supported_union_field(
    items: &ItemTree,
    sigs: &Signatures,
    ty: Ty,
    checked: &mut FxHashMap<Ty, bool>,
    depth: u32,
) -> bool {
    let ty = traits::normalize(items, sigs, &[], ty);
    if ty.has_params() || ty.has_infer() || depth >= 128 {
        return false;
    }
    if let Some(&supported) = checked.get(&ty) {
        return supported;
    }
    // A repeated in-progress type is a cycle. Memoize finished types too,
    // so repeated fields do not expand the same representation exponentially.
    // The depth bound also stops expanding generic cycles such as S<S<T>>.
    checked.insert(ty, false);
    let supported = match ty.kind() {
        TyKind::Int(_) | TyKind::Float(_) | TyKind::Bool | TyKind::Char => true,
        TyKind::Tuple(fields) => fields
            .iter()
            .all(|&field| supported_union_field(items, sigs, field, checked, depth + 1)),
        // A raw pointer stores only its address. Pointee fields may be
        // recursive and need no expansion; metadata and region slots still
        // have to fit the region-free representation of this union subset.
        TyKind::RawPtr(_, pointee) => regions::slots(items, ty) == 0 && !sigs.is_unsized(pointee),
        // Function pointers carry no borrowed value. Their parameter references
        // are late-bound; signature resolution rejects named lifetimes and
        // reference returns that would require tracking regions in the pointer.
        // Do not recurse into the signature as though its inputs were storage.
        TyKind::FnPtr(_) => true,
        TyKind::Array(elem, len) => {
            len.as_usize().is_some() && supported_union_field(items, sigs, elem, checked, depth + 1)
        }
        TyKind::Adt(adt, args) if !is_lang_struct(items, adt.strukt, "manually_drop") => {
            // Region slots also occur in type arguments, even when they are
            // phantom. Check these with the same memoized traversal rather
            // than expanding a shared generic type's region slots repeatedly.
            items.structs[adt.strukt].generics.lifetimes.is_empty()
                && args.iter().all(|&arg| {
                    matches!(arg.kind(), TyKind::Value(_))
                        || supported_union_field(items, sigs, arg, checked, depth + 1)
                })
                && sigs.contents(ty).is_some_and(|fields| {
                    fields
                        .iter()
                        .all(|&field| supported_union_field(items, sigs, field, checked, depth + 1))
                })
        }
        _ => false,
    };
    checked.insert(ty, supported);
    supported
}
