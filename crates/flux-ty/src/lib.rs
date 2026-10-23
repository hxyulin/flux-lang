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

mod impl_index;
mod in_scope;
mod infer;
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
pub use traits::{Predicate, Projection, TraitRef, TraitSig};
pub use ty::{
    AdtId, AliasTy, ClosureArgs, ClosureId, ClosureKind, DynTy, FnDefTy, FnSigTy, GenericArgs,
    InferVar, IntTy, Mutability, OpaqueTy, ParamTy, Ty, TyKind, subst_list, symbol_list,
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
            return Some(!p.self_ty.is_unsized());
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
                let args = ty::map_list(alias.args, &mut go);
                let alias = ty::AliasTy { args, ..alias };
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
                    let needed = projections
                        .iter()
                        .map(|p| max_param(p.ty))
                        .chain([max_param(t)])
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
                    break 'alias go(t.subst(&binds));
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
        TyKind::Alias(alias) => alias.args.iter().map(|&t| max_param(t)).max().unwrap_or(0),
        TyKind::Opaque(opaque) => opaque.args.iter().map(|&t| max_param(t)).max().unwrap_or(0),
        TyKind::Ref(_, t) | TyKind::RawPtr(_, t) | TyKind::Slice(t) => max_param(t),
        TyKind::Array(t, len) => max_param(t).max(max_param(len)),
        TyKind::FnDef(_) | TyKind::FnPtr(_) => ty.children().map(max_param).max().unwrap_or(0),
        _ => 0,
    }
}

/// What the names in a type mean besides items: `Self`, if anything,
/// and the generic parameters in scope, by index.
#[derive(Debug, Clone, Copy, Default)]
pub struct TypeScope<'a> {
    pub self_ty: Option<Ty>,
    pub generics: &'a [Symbol],
    /// The type of each of `generics` that is a const parameter, `None`
    /// for a type parameter ([`generic_consts`]); empty where they are
    /// not known, and then each is taken for what its use needs.
    pub consts: &'a [Option<Ty>],
    /// In an item's signature, where `_` is not allowed, what rustc
    /// calls such items ("functions"); `None` in a body, where `_` is a
    /// type to infer.
    pub signature_of: Option<&'static str>,
    /// The bounds in scope, with what they imply: where `T::Out` finds
    /// its trait.
    pub bounds: &'a [Predicate],
    /// In a trait's `impl` block, its trait: `Self::Out` is its type.
    pub self_trait: Option<TraitRef>,
    /// Resolved supertrait edges for inherited `Self::Out` in an impl.
    /// Keeping these separate from `bounds` avoids treating the impl being
    /// checked as an assumption when proving its declaration obligations.
    pub self_supertraits: &'a [Predicate],
    pub self_inherited_types: &'a [(TraitRef, Symbol, Ty)],
    /// In a trait's `impl` block's items, its associated types: what
    /// `Self::Out` and `<Self as Tr>::Out` are, as rustc normalizes them
    /// there (the block is the one for its `Self`).
    pub self_types: &'a [(Symbol, Ty)],
    /// The defaults of traits' parameters, for bounds that leave them
    /// out.
    pub defaults: Option<&'a traits::TraitDefaults>,
    /// The values of the constants array lengths name, where known.
    pub lens: Option<&'a ArrayLens<'a>>,
    /// In a function's signature, the function, whose opaque types its
    /// return type names.
    pub function: Option<FunctionId>,
    /// Which of `generics` have no implicit `Sized` bound
    /// ([`unsized_params`]); empty where all do.
    pub unsized_params: &'a [bool],
}

/// The values of the constants that array lengths name (`[u8; N]`), as
/// far as they are evaluated: `None` for one whose evaluation failed,
/// which is reported where it is evaluated. Type checking needs them,
/// and evaluating them needs MIR, so [`check_file_with`] gets them
/// from a function that builds it.
#[derive(Debug)]
pub struct ArrayLens<'a> {
    pub values: &'a LenValues,
    /// The constants named whose values are not in `values`, as types
    /// were resolved: what to evaluate.
    pub missing: std::cell::RefCell<Vec<ConstId>>,
    /// The types of the anonymous constants of const generic arguments
    /// (`f::<{ N + 1 }>`), their parameters', as far as known.
    pub arg_tys: &'a ArgTys,
    /// Those found as types were resolved that `arg_tys` lacks.
    pub new_arg_tys: std::cell::RefCell<Vec<(ConstId, Ty)>>,
}

/// The types of the anonymous constants of const generic arguments.
pub type ArgTys = rustc_hash::FxHashMap<ConstId, Ty>;

/// The evaluated values of constants that array lengths and const
/// generic arguments name, as bits (`1` for `true`).
pub type LenValues = rustc_hash::FxHashMap<ConstId, Option<u128>>;

/// Evaluates the constants array lengths name, given the signatures
/// so far (whose [`Signatures::array_lens`] it may extend as it needs):
/// each constant's value, `None` if its evaluation failed, with the
/// diagnostics that say why.
pub type LenEvaluator<'e> =
    &'e dyn Fn(&Hir, &Signatures, &[ConstId]) -> (Vec<(ConstId, Option<u128>)>, Vec<Diagnostic>);

impl<'a> ArrayLens<'a> {
    pub fn new(values: &'a LenValues, arg_tys: &'a ArgTys) -> ArrayLens<'a> {
        ArrayLens {
            values,
            missing: Default::default(),
            arg_tys,
            new_arg_tys: Default::default(),
        }
    }

    /// The type of `konst`, the anonymous constant of a const generic
    /// argument, if known.
    pub fn arg_ty(&self, konst: ConstId) -> Option<Ty> {
        self.arg_tys.get(&konst).copied()
    }

    /// Notes that `konst`, the anonymous constant of a const generic
    /// argument, has type `ty`.
    fn found_arg_ty(&self, konst: ConstId, ty: Ty) {
        if ty != Ty::ERROR && !self.arg_tys.contains_key(&konst) {
            let mut new = self.new_arg_tys.borrow_mut();
            if !new.iter().any(|&(k, _)| k == konst) {
                new.push((konst, ty));
            }
        }
    }

    /// The value of constant `konst`, if known; one not evaluated yet is
    /// noted as missing.
    fn value(&self, konst: ConstId) -> Option<u128> {
        match self.values.get(&konst) {
            Some(value) => *value,
            None => {
                let mut missing = self.missing.borrow_mut();
                if !missing.contains(&konst) {
                    missing.push(konst);
                }
                None
            }
        }
    }
}

impl TypeScope<'_> {
    /// Whether values of `ty` may have no size known at compile time
    /// here ([`maybe_unsized`]).
    pub fn is_unsized(&self, items: &ItemTree, ty: Ty) -> bool {
        maybe_unsized(items, ty, self.unsized_params, self.bounds)
    }

    /// The generic parameter called `name`, if one is in scope.
    pub fn param(&self, name: Symbol) -> Option<Ty> {
        let index = self.generics.iter().rposition(|&n| n == name)?;
        Some(Ty::param(index as u32, name))
    }

    /// The type of the const parameter called `name`, if it is one in
    /// scope (an error if that is not known).
    pub fn const_param(&self, name: Symbol) -> Option<Ty> {
        let index = self.generics.iter().rposition(|&n| n == name)?;
        match self.consts.get(index) {
            Some(ty) => *ty,
            None => Some(Ty::ERROR),
        }
    }

    /// Whether `name` is a type parameter in scope, known not to be a
    /// const one.
    fn type_param(&self, name: Symbol) -> bool {
        self.generics
            .iter()
            .rposition(|&n| n == name)
            .is_some_and(|i| matches!(self.consts.get(i), Some(None)))
    }
}

/// Which generic parameters of an item in `owner` with its own
/// `generics` have no implicit `Sized` bound, in the order of
/// [`generic_names`]: a trait's `Self`, and those bounded `?Sized`.
pub fn unsized_params(items: &ItemTree, owner: Option<Owner>, own: &Generics) -> Vec<bool> {
    let mut out = Vec::new();
    if let Some(Owner::Trait(_)) = owner {
        out.push(true);
    }
    out.extend(
        items
            .owner_generics(owner)
            .into_iter()
            .chain([own])
            .flat_map(|g| g.params.iter().map(|p| p.maybe_unsized)),
    );
    out
}

/// Whether values of `ty` may have no size known at compile time: a
/// slice, `str`, or a generic parameter without an implicit `Sized`
/// bound (`unsized_params`, by index) that none of `bounds` says is
/// `Sized`.
pub fn maybe_unsized(
    items: &ItemTree,
    ty: Ty,
    unsized_params: &[bool],
    bounds: &[Predicate],
) -> bool {
    match ty.kind() {
        TyKind::Param(p) => {
            unsized_params.get(p.index as usize) == Some(&true)
                && !bounds.iter().any(|b| {
                    b.self_ty == ty && items.traits[b.trait_ref.trait_].lang == Some(sym::sized)
                })
        }
        _ => ty.is_unsized(),
    }
}

/// The types of the const parameters of an item in `owner` with its own
/// `generics`, `None` for its type parameters, in the order of
/// [`generic_names`].
pub fn generic_consts(items: &ItemTree, owner: Option<Owner>, own: &Generics) -> Vec<Option<Ty>> {
    let mut consts = Vec::new();
    if let Some(Owner::Trait(_)) = owner {
        consts.push(None);
    }
    consts.extend(
        items
            .owner_generics(owner)
            .into_iter()
            .chain([own])
            .flat_map(|g| g.params.iter().map(param_const_ty)),
    );
    consts
}

/// The type of a const parameter, `None` for a type parameter.
pub fn param_const_ty(param: &flux_hir::GenericParam) -> Option<Ty> {
    match &param.kind {
        flux_hir::ParamKind::Type => None,
        flux_hir::ParamKind::Const { ty, .. } => Some(const_param_ty(ty)),
    }
}

/// The type of a const parameter written as `ty`: an integer type,
/// `bool` or `char` ([`check_const_params`] reports others), or an
/// error.
pub fn const_param_ty(ty: &TypeRef) -> Ty {
    let TypeRef::Path(path) = ty else {
        return Ty::ERROR;
    };
    path.as_name()
        .and_then(|name| primitive(name).ok())
        .filter(|t| t.as_int().is_some() || *t == Ty::BOOL || *t == Ty::CHAR)
        .unwrap_or(Ty::ERROR)
}

/// rustc's errors for the types of the const parameters of `generics`,
/// in `module` with the generic parameters `names` in scope: one that
/// names a generic parameter (E0770), and one that is not an integer
/// type, `bool` or `char`.
pub fn check_const_params(
    items: &ItemTree,
    module: ModuleId,
    generics: &Generics,
    names: &[Symbol],
    errors: &mut Vec<(TextRange, String)>,
) {
    for param in generics.params.iter() {
        let flux_hir::ParamKind::Const { ty, ty_range } = &param.kind else {
            continue;
        };
        let mut generic = false;
        visit_paths(ty, &mut |path| {
            if let Some(&first) = path.segments.first()
                && path.anchor == flux_hir::PathAnchor::Plain
                && (names.contains(&first) || first == kw::SelfUpper)
            {
                generic = true;
            }
        });
        if generic {
            errors.push((
                *ty_range,
                "the type of const parameters must not depend on other generic parameters"
                    .to_owned(),
            ));
            continue;
        }
        if const_param_ty(ty) != Ty::ERROR {
            continue;
        }
        // The other primitive types, which flux lacks or which are not
        // sized.
        if let TypeRef::Path(path) = ty
            && let Some(name) = path.as_name()
            && matches!(name.as_str(), "f16" | "f32" | "f64" | "f128" | "str")
        {
            errors.push((
                *ty_range,
                format!("`{name}` is forbidden as the type of a const generic parameter"),
            ));
            continue;
        }
        let resolved = resolve_type(
            items,
            module,
            ty,
            TypeScope::default(),
            *ty_range,
            &mut Vec::new(),
        );
        if resolved != Ty::ERROR
            && resolved.as_int().is_none()
            && resolved != Ty::BOOL
            && resolved != Ty::CHAR
        {
            errors.push((
                *ty_range,
                format!("`{ty}` is forbidden as the type of a const generic parameter"),
            ));
        } else if resolved != Ty::ERROR {
            errors.push((
                *ty_range,
                "const parameters of types named through aliases are not supported yet".to_owned(),
            ));
        } else {
            // Reported where the type is resolved.
            resolve_type(items, module, ty, TypeScope::default(), *ty_range, errors);
        }
    }
}

/// The generic argument `arg`, written at `range`, for `param`: a type
/// for a type parameter, a constant ([`TyKind::Value`] or a const
/// parameter) for a const one. A path is a type if it names one, and
/// otherwise a constant, as rustc reads it.
#[allow(clippy::too_many_arguments)]
pub fn resolve_generic_arg(
    items: &ItemTree,
    module: ModuleId,
    param: Option<&flux_hir::GenericParam>,
    arg: &TypeRef,
    scope: TypeScope<'_>,
    range: TextRange,
    errors: &mut Vec<(TextRange, String)>,
) -> Ty {
    let want = match param.map(|p| &p.kind) {
        Some(flux_hir::ParamKind::Const { ty, .. }) => const_param_ty(ty),
        _ => {
            if let TypeRef::Const(_) = arg {
                errors.push((
                    range,
                    "constant provided when a type was expected".to_owned(),
                ));
                return Ty::ERROR;
            }
            return resolve_type(items, module, arg, scope, range, errors);
        }
    };
    let value = match arg {
        TypeRef::Const(value) => value.clone(),
        TypeRef::Infer if scope.signature_of.is_none() => {
            return Ty::infer(ty::InferVar::PLACEHOLDER);
        }
        TypeRef::Path(path) if names_type(items, module, path, scope) != Some(true) => {
            flux_hir::ArrayLen {
                kind: flux_hir::ArrayLenKind::Path(path.clone()),
                range,
            }
        }
        _ => {
            errors.push((
                range,
                "type provided when a constant was expected".to_owned(),
            ));
            return Ty::ERROR;
        }
    };
    const_value(items, module, &value, want, scope, errors).unwrap_or(Ty::ERROR)
}

/// Whether `path`, a generic argument, names a type rather than a
/// constant; `None` for a generic parameter of a kind not known.
fn names_type(
    items: &ItemTree,
    module: ModuleId,
    path: &Path,
    scope: TypeScope<'_>,
) -> Option<bool> {
    if path.qself.is_some() || path.args.iter().any(Option::is_some) {
        return Some(true);
    }
    if let Some(name) = path.as_name() {
        if let Some(index) = scope.generics.iter().rposition(|&n| n == name) {
            return scope.consts.get(index).map(Option::is_none);
        }
        if name == kw::SelfUpper {
            return Some(true);
        }
    }
    if path.segments.len() > 1
        && path.anchor == flux_hir::PathAnchor::Plain
        && (path.segments[0] == kw::SelfUpper || scope.generics.contains(&path.segments[0]))
    {
        // `Self::N`, `T::Out`: an associated constant or type, as the
        // parameter wants.
        return None;
    }
    Some(items.resolve(module, path, Ns::Types).is_ok())
}

/// The generic arguments `written` (with their ranges) for parameters
/// `params`, as [`resolve_generic_arg`] resolves each.
pub fn resolve_generic_args(
    items: &ItemTree,
    module: ModuleId,
    params: &[flux_hir::GenericParam],
    written: &flux_hir::GenericArgs,
    scope: TypeScope<'_>,
    errors: &mut Vec<(TextRange, String)>,
) -> Vec<Ty> {
    // rustc reports only the first argument of the wrong kind (E0747).
    let wrong_kind = written
        .types
        .iter()
        .zip(&written.ranges)
        .zip(params)
        .find_map(|((arg, &range), param)| {
            let is_const = match arg {
                TypeRef::Const(_) => true,
                TypeRef::Path(path) => !names_type(items, module, path, scope)?,
                TypeRef::Infer => return None,
                _ => false,
            };
            match (param.is_const(), is_const) {
                (false, true) => Some((range, "constant provided when a type was expected")),
                (true, false) => Some((range, "type provided when a constant was expected")),
                _ => None,
            }
        });
    if let Some((range, message)) = wrong_kind {
        errors.push((range, message.to_owned()));
        return vec![Ty::ERROR; written.types.len()];
    }
    written
        .types
        .iter()
        .zip(&written.ranges)
        .enumerate()
        .map(|(i, (arg, &range))| {
            resolve_generic_arg(items, module, params.get(i), arg, scope, range, errors)
        })
        .collect()
}

/// The names of the generic parameters of an item in `owner` with its
/// own `generics`: an `impl` block's, or a trait's `Self` and its own,
/// then the item's.
pub fn generic_names(items: &ItemTree, owner: Option<Owner>, own: &Generics) -> Vec<Symbol> {
    let mut names = Vec::new();
    if let Some(Owner::Trait(_)) = owner {
        names.push(kw::SelfUpper);
    }
    names.extend(
        items
            .owner_generics(owner)
            .into_iter()
            .chain([own])
            .flat_map(|g| g.params.iter().map(|p| p.name)),
    );
    names
}

/// "1 generic argument", "2 generic arguments".
fn generic_arguments(n: usize) -> String {
    format!("{n} generic argument{}", if n == 1 { "" } else { "s" })
}

/// rustc's error for `got` generic arguments where a `kind` (struct,
/// function, ...) takes `want`.
pub fn wrong_arg_count(kind: &str, want: usize, got: usize) -> String {
    let verb = if got == 1 { "was" } else { "were" };
    format!(
        "{kind} takes {} but {} {verb} supplied",
        generic_arguments(want),
        generic_arguments(got)
    )
}

/// Resolves a type as written in `module`: a struct, a primitive type,
/// a generic parameter, or `Self`. Errors go to `errors`, at the part of
/// the type they are about within `range`, and make the type an error
/// unless they are about privacy.
pub fn resolve_type(
    items: &ItemTree,
    module: ModuleId,
    ty: &TypeRef,
    scope: TypeScope<'_>,
    range: TextRange,
    errors: &mut Vec<(TextRange, String)>,
) -> Ty {
    let mutability = |mutable: bool| {
        if mutable {
            Mutability::Mut
        } else {
            Mutability::Not
        }
    };
    let fail = |errors: &mut Vec<(TextRange, String)>, at: TextRange, message: String| {
        errors.push((at, message));
        Ty::ERROR
    };
    match ty {
        TypeRef::Path(path) => {
            let last = path.segments.len().saturating_sub(1);
            // Generic arguments are only for the type itself, not the
            // modules before it.
            for i in 0..last {
                if let Some(args) = path.args(i) {
                    let name = path.segments[i];
                    errors.push((
                        args.ranges.first().copied().unwrap_or(args.range),
                        format!("type arguments are not allowed on module `{name}`"),
                    ));
                }
            }
            let args = path.args(last);
            // Arguments to something that takes none.
            let no_args = |errors: &mut Vec<(TextRange, String)>, what: String| {
                if let Some(args) = args {
                    errors.push((
                        args.ranges.first().copied().unwrap_or(args.range),
                        format!("type arguments are not allowed on {what}"),
                    ));
                }
            };
            if let Some(name) = path.as_name()
                && let Some(param) = scope.param(name)
            {
                if scope.const_param(name).is_some()
                    && !scope.type_param(name)
                    && !scope.consts.is_empty()
                {
                    return fail(
                        errors,
                        range,
                        format!("cannot find type `{name}` in this scope"),
                    );
                }
                no_args(errors, format!("type parameter `{name}`"));
                return param;
            }
            // `T::Out`, `Self::Out`, `<T as Tr>::Out`.
            let first = path.segments.first().copied();
            let relative = path.anchor == flux_hir::PathAnchor::Plain
                && path.segments.len() > 1
                && first.is_some_and(|f| scope.param(f).is_some() || f == kw::SelfUpper);
            if path.qself.is_some() || relative {
                return assoc_type(items, module, path, scope, range, errors);
            }
            let resolved = match items.resolve(module, path, Ns::Types) {
                Err(e @ ResolveError::Private { .. }) => {
                    let ResolveError::Private {
                        message, resolved, ..
                    } = &e
                    else {
                        unreachable!()
                    };
                    errors.push((e.range(path, range), message.clone()));
                    Ok(*resolved)
                }
                result => result,
            };
            let last_range = path.range(last, range);
            match resolved {
                Ok(Resolved::SelfType) => {
                    no_args(errors, "self type".to_owned());
                    match scope.self_ty {
                        Some(ty) => ty,
                        None if outer_generic(items, module, kw::SelfUpper) => {
                            fail(errors, range, "can't use `Self` from outer item".to_owned())
                        }
                        None => fail(
                            errors,
                            range,
                            "cannot find type `Self` in this scope".to_owned(),
                        ),
                    }
                }
                Ok(Resolved::Def(Def::Struct(id))) => {
                    struct_type(items, module, id, path, scope, range, errors)
                }
                Ok(Resolved::Def(Def::TypeAlias(id))) => {
                    alias_type(items, module, id, path, scope, range, errors)
                }
                Ok(Resolved::Primitive(name)) => {
                    no_args(errors, format!("builtin type `{name}`"));
                    match primitive(name) {
                        Ok(ty) => ty,
                        Err(message) => fail(errors, range, message),
                    }
                }
                // A bare trait is a trait object in edition 2015 only.
                Ok(Resolved::Def(Def::Trait(_))) => fail(
                    errors,
                    last_range,
                    "expected a type, found a trait".to_owned(),
                ),
                Ok(Resolved::Def(Def::Module(m))) => fail(
                    errors,
                    last_range,
                    format!("expected type, found module `{}`", items.modules[m].name),
                ),
                Ok(Resolved::Def(Def::Variant(..))) => fail(
                    errors,
                    range,
                    format!("expected type, found variant `{path}`"),
                ),
                Ok(_) => fail(errors, last_range, format!("expected type, found `{path}`")),
                Err(e @ ResolveError::NotFound { .. }) => {
                    let module_of_use = module;
                    let ResolveError::NotFound { segment, module } = e else {
                        unreachable!()
                    };
                    let name = path.segments[segment];
                    if module.is_none() && outer_generic(items, module_of_use, name) {
                        return fail(
                            errors,
                            e.range(path, range),
                            "can't use generic parameters from outer item".to_owned(),
                        );
                    }
                    let place = match module {
                        Some(module) => items.in_module(module),
                        None => "this scope".to_owned(),
                    };
                    fail(
                        errors,
                        e.range(path, range),
                        format!("cannot find type `{name}` in {place}"),
                    )
                }
                Err(ResolveError::Reported) => Ty::ERROR,
                Err(e) => {
                    let at = e.range(path, range);
                    let (ResolveError::Failed { message, .. }
                    | ResolveError::Private { message, .. }) = e
                    else {
                        unreachable!()
                    };
                    fail(errors, at, message)
                }
            }
        }
        TypeRef::Tuple(fields) => {
            let fields: Vec<Ty> = fields
                .iter()
                .map(|f| resolve_type(items, module, f, scope, range, errors))
                .collect();
            if fields.contains(&Ty::ERROR) {
                return Ty::ERROR;
            }
            if let Some(&field) = fields.iter().find(|&&f| scope.is_unsized(items, f)) {
                return fail(errors, range, unsized_error(field));
            }
            Ty::tuple(&fields)
        }
        TypeRef::Never => Ty::NEVER,
        TypeRef::FnPtr {
            params,
            ret,
            is_unsafe,
        } => {
            let params: Vec<Ty> = params
                .iter()
                .map(|p| resolve_type(items, module, p, scope, range, errors))
                .collect();
            let ret = resolve_type(items, module, ret, scope, range, errors);
            if params.contains(&Ty::ERROR) || ret == Ty::ERROR {
                return Ty::ERROR;
            }
            Ty::new(TyKind::FnPtr(FnSigTy::new(&params, ret, *is_unsafe)))
        }
        TypeRef::Ptr { mutable, pointee } => {
            let pointee = resolve_type(items, module, pointee, scope, range, errors);
            if pointee == Ty::ERROR {
                return Ty::ERROR;
            }
            Ty::raw_ptr(mutability(*mutable), pointee)
        }
        TypeRef::Ref {
            mutable, pointee, ..
        } => {
            let pointee = resolve_type(items, module, pointee, scope, range, errors);
            if pointee == Ty::ERROR {
                return Ty::ERROR;
            }
            Ty::new(TyKind::Ref(mutability(*mutable), pointee))
        }
        TypeRef::Array { elem, len } => {
            let elem = resolve_type(items, module, elem, scope, range, errors);
            let len = array_len(items, module, len, scope, errors);
            match (elem, len) {
                (Ty::ERROR, _) | (_, None) => Ty::ERROR,
                (elem, _) if scope.is_unsized(items, elem) => {
                    fail(errors, range, unsized_error(elem))
                }
                (elem, Some(len)) => Ty::array_of(elem, len),
            }
        }
        TypeRef::Const(_) => fail(
            errors,
            range,
            "constant provided when a type was expected".to_owned(),
        ),
        TypeRef::Slice(elem) => {
            let elem = resolve_type(items, module, elem, scope, range, errors);
            if elem == Ty::ERROR {
                return Ty::ERROR;
            }
            if scope.is_unsized(items, elem) {
                return fail(errors, range, unsized_error(elem));
            }
            Ty::slice(elem)
        }
        TypeRef::ImplParam(index) => match scope.generics.get(*index as usize) {
            Some(&name) => Ty::param(*index, name),
            None => Ty::ERROR,
        },
        TypeRef::Opaque(index) => {
            let Some(function) = scope.function else {
                return Ty::ERROR;
            };
            let Some(decl) = items.functions[function].opaques.get(*index as usize) else {
                return Ty::ERROR;
            };
            Ty::opaque(OpaqueTy {
                function,
                index: *index,
                name: decl.name,
                args: Ty::list(&params_of_names(scope.generics)),
                lifetimes: 0,
            })
        }
        TypeRef::Dyn(d) => objects::resolve_dyn(items, module, d, scope, range, errors),
        // One lowering did not make something else, which is in a path
        // in a body (`Option::<impl Copy>::None`), as rustc names it.
        TypeRef::ImplTrait(it) => fail(
            errors,
            it.range,
            "`impl Trait` is not allowed in paths".to_owned(),
        ),
        TypeRef::Infer => match scope.signature_of {
            Some(items) => fail(
                errors,
                range,
                format!(
                    "the placeholder `_` is not allowed within types on item signatures for {items}"
                ),
            ),
            None => Ty::infer(ty::InferVar::PLACEHOLDER),
        },
        TypeRef::Error => Ty::ERROR,
    }
}

/// The value of an array's length, `len`, written in `module`: a
/// constant of type `usize` ([`const_value`]).
pub fn array_len(
    items: &ItemTree,
    module: ModuleId,
    len: &flux_hir::ArrayLen,
    scope: TypeScope<'_>,
    errors: &mut Vec<(TextRange, String)>,
) -> Option<Ty> {
    const_value(items, module, len, Ty::int(IntTy::Usize), scope, errors)
}

/// The value of a constant written in a type, `len` (an array's length
/// or a const generic argument), of type `want`, in `module`: a
/// [`TyKind::Value`], or a const parameter. `None` if it is not known
/// (reported unless it is waiting for evaluation).
pub fn const_value(
    items: &ItemTree,
    module: ModuleId,
    len: &flux_hir::ArrayLen,
    want: Ty,
    scope: TypeScope<'_>,
    errors: &mut Vec<(TextRange, String)>,
) -> Option<Ty> {
    let range = len.range;
    let mismatch = |errors: &mut Vec<(TextRange, String)>, found: &dyn std::fmt::Display| {
        errors.push((
            range,
            format!("mismatched types: expected `{want}`, found {found}"),
        ));
        None
    };
    let value = |bits: u128| Some(Ty::value(want, bits));
    let konst = match &len.kind {
        flux_hir::ArrayLenKind::Error => return None,
        flux_hir::ArrayLenKind::Const(konst) => *konst,
        flux_hir::ArrayLenKind::NegLit {
            value: magnitude,
            suffix,
        } => {
            let Some(int) = want.as_int() else {
                return mismatch(errors, &"integer");
            };
            if let Some(suffix) = suffix
                && *suffix != int.name()
            {
                return mismatch(errors, &format!("`{suffix}`"));
            }
            if !int.is_signed() || *magnitude > int.max() + 1 {
                errors.push((range, format!("literal out of range for `{want}`")));
                return None;
            }
            let mask = u128::MAX >> (128 - int.bits());
            return value(magnitude.wrapping_neg() & mask);
        }
        flux_hir::ArrayLenKind::Anon(file, node) => {
            // One that uses generic parameters is reported, and has none.
            let konst = *items.anon_consts.get(&(*file, *node))?;
            let lens = scope.lens?;
            if items.consts[konst].anon.is_some_and(|a| a.is_arg) {
                lens.found_arg_ty(konst, want);
            }
            return lens.value(konst).and_then(value);
        }
        flux_hir::ArrayLenKind::Lit {
            value: bits,
            suffix,
        } => {
            let Some(int) = want.as_int() else {
                if want == Ty::ERROR {
                    return None;
                }
                return mismatch(errors, &"integer");
            };
            return match suffix {
                Some(suffix) if *suffix != int.name() => mismatch(errors, &format!("`{suffix}`")),
                _ if *bits > int.max() => {
                    errors.push((range, format!("literal out of range for `{want}`")));
                    None
                }
                _ => value(*bits),
            };
        }
        flux_hir::ArrayLenKind::Bool(b) => {
            if want != Ty::BOOL {
                return mismatch(errors, &"`bool`");
            }
            return value(u128::from(*b));
        }
        flux_hir::ArrayLenKind::Char(c) => {
            if want != Ty::CHAR {
                return mismatch(errors, &"`char`");
            }
            return value(u128::from(*c));
        }
        flux_hir::ArrayLenKind::Path(path)
            if let Some(name) = path.as_name()
                && let Some(ty) = scope.const_param(name) =>
        {
            if scope.type_param(name) {
                errors.push((
                    range,
                    format!("expected value, found type parameter `{name}`"),
                ));
                return None;
            }
            if ty != want && ty != Ty::ERROR && want != Ty::ERROR {
                return mismatch(errors, &format!("`{ty}`"));
            }
            return scope.param(name);
        }
        flux_hir::ArrayLenKind::Path(path)
            if path.segments.first() == Some(&kw::SelfUpper)
                && scope.self_ty.is_some_and(Ty::has_params) =>
        {
            errors.push((
                path.range(0, range),
                "generic `Self` types are currently not permitted in anonymous constants"
                    .to_owned(),
            ));
            return None;
        }
        flux_hir::ArrayLenKind::Path(path) => match items.resolve(module, path, Ns::Values) {
            Ok(Resolved::Def(Def::Const(konst))) => konst,
            // A constant of an `impl` block without generic parameters.
            Ok(Resolved::AssocConst(konst))
                if items.consts[konst]
                    .owner
                    .and_then(Owner::as_impl)
                    .is_some_and(|imp| items.impls[imp].generics.is_empty()) =>
            {
                konst
            }
            Err(ResolveError::Reported) => return None,
            Err(ResolveError::NotFound { segment, .. }) if path.segments.len() == 1 => {
                let name = path.segments[segment];
                errors.push((range, format!("cannot find value `{name}` in this scope")));
                return None;
            }
            _ => {
                errors.push((
                    range,
                    "array lengths other than integers and constants are not supported yet"
                        .to_owned(),
                ));
                return None;
            }
        },
    };
    let data = &items.consts[konst];
    let ty = resolve_type(
        items,
        data.loc.module,
        &data.ty,
        TypeScope::default(),
        data.ty_range,
        &mut Vec::new(),
    );
    if ty != want {
        if ty == Ty::ERROR || want == Ty::ERROR {
            return None;
        }
        return mismatch(errors, &format!("`{ty}`"));
    }
    scope.lens?.value(konst).and_then(value)
}

/// Whether `name` is a generic parameter, or `Self`, of a function whose
/// body has the block `module` is, or is in: which an item declared
/// there cannot use (E0401).
fn outer_generic(items: &ItemTree, module: ModuleId, name: Symbol) -> bool {
    let mut module = Some(module);
    while let Some(m) = module {
        let data = &items.modules[m];
        module = data.parent;
        let Some(f) = data.block.and_then(|b| b.function) else {
            continue;
        };
        let f = &items.functions[f];
        let found = if name == kw::SelfUpper {
            f.owner.is_some()
        } else {
            generic_names(items, f.owner, &f.generics).contains(&name)
        };
        if found {
            return true;
        }
    }
    false
}

/// An associated type named by a path: `T::Out`, `Self::Out`,
/// `<T as Tr>::Out` or `<T>::Out`.
fn assoc_type(
    items: &ItemTree,
    module: ModuleId,
    path: &Path,
    scope: TypeScope<'_>,
    range: TextRange,
    errors: &mut Vec<(TextRange, String)>,
) -> Ty {
    let (self_ty, trait_path, name_index) = match &path.qself {
        Some(qself) => {
            let ty = resolve_type(items, module, &qself.ty, scope, qself.ty_range, errors);
            (ty, qself.trait_.as_ref().map(|t| (t, qself.range)), 0)
        }
        None => {
            let first = Path {
                segments: path.segments[..1].into(),
                ranges: path.ranges[..1.min(path.ranges.len())].into(),
                args: path.args[..1.min(path.args.len())].into(),
                ..path.clone()
            };
            let ty = resolve_type(items, module, &TypeRef::Path(first), scope, range, errors);
            (ty, None, 1)
        }
    };
    if path.segments.len() != name_index + 1 {
        errors.push((
            range,
            "associated types of associated types are not supported yet".into(),
        ));
        return Ty::ERROR;
    }
    if self_ty == Ty::ERROR {
        return Ty::ERROR;
    }
    let name = path.segments[name_index];
    let at = path.range(name_index, range);
    if let Some(args) = path.args(name_index) {
        let at = args.ranges.first().copied().unwrap_or(args.range);
        errors.push((at, "generic associated types are not supported yet".into()));
        return Ty::ERROR;
    }
    let has_type = |t: flux_hir::TraitId| items.traits[t].types.iter().any(|ty| ty.name == name);
    // `<T as Tr>::Out`: the trait is written.
    if let Some((trait_path, qrange)) = trait_path {
        let bound = flux_hir::Bound {
            path: trait_path.clone(),
            range: qrange,
        };
        let Some((trait_ref, _)) =
            traits::resolve_bound(items, module, &bound, self_ty, scope, errors)
        else {
            return Ty::ERROR;
        };
        if !has_type(trait_ref.trait_) {
            errors.push((
                at,
                format!(
                    "cannot find associated type `{name}` in trait `{}`",
                    trait_ref.name
                ),
            ));
            return Ty::ERROR;
        }
        let self_written = path.qself.as_ref().is_some_and(
            |q| matches!(&q.ty, TypeRef::Path(p) if p.as_name() == Some(kw::SelfUpper)),
        );
        if self_written
            && Some(trait_ref) == scope.self_trait
            && let Some(&(_, ty)) = scope.self_types.iter().find(|(n, _)| *n == name)
        {
            return ty;
        }
        // AliasTy has no trait-region table. Reject these projections until
        // normalization can preserve their owner and type-argument regions.
        if !items.traits[trait_ref.trait_].generics.lifetimes.is_empty()
            || trait_ref
                .args
                .iter()
                .any(|&t| regions::slots(items, t) != 0)
        {
            errors.push((
                qrange,
                "lifetimes in qualified trait projections are not supported yet".into(),
            ));
            return Ty::ERROR;
        }
        return Ty::alias(traits::alias_of(trait_ref, self_ty, name));
    }
    // `Self::Out` in a trait's `impl` block: the block's own.
    // (Only written `Self`: `I::Item` in `impl<I: It> Tr for I` is `I`'s
    // bounds', as in rustc.)
    let written_self = path.qself.is_none() && path.segments[0] == kw::SelfUpper;
    if let Some(trait_ref) = scope.self_trait
        && Some(self_ty) == scope.self_ty
        && (written_self || !matches!(self_ty.kind(), TyKind::Param(_)))
    {
        if has_type(trait_ref.trait_) {
            if written_self
                && let Some(&(_, ty)) = scope.self_types.iter().find(|(n, _)| *n == name)
            {
                return ty;
            }
            return Ty::alias(traits::alias_of(trait_ref, self_ty, name));
        }
        if written_self {
            let found: Vec<_> = scope
                .self_supertraits
                .iter()
                .filter(|p| has_type(p.trait_ref.trait_))
                .map(|p| p.trait_ref)
                .collect();
            match found.as_slice() {
                [inherited] => {
                    // AliasTy has no table for erased trait-argument regions.
                    // Preserve the qualified-projection boundary here too.
                    if !items.traits[inherited.trait_].generics.lifetimes.is_empty()
                        || inherited
                            .args
                            .iter()
                            .any(|&t| regions::slots(items, t) != 0)
                    {
                        errors.push((
                            at,
                            "lifetimes in inherited trait projections are not supported yet".into(),
                        ));
                        return Ty::ERROR;
                    }
                    if let Some(&(_, _, ty)) = scope
                        .self_inherited_types
                        .iter()
                        .find(|(tr, n, _)| tr == inherited && *n == name)
                    {
                        if ty.has_aliases() {
                            errors.push((
                                at,
                                "nested inherited associated types are not supported yet".into(),
                            ));
                            return Ty::ERROR;
                        }
                        if scope.signature_of != Some("functions") && regions::slots(items, ty) != 0
                        {
                            errors.push((at, "region-bearing inherited associated types outside function signatures are not supported yet".into()));
                            return Ty::ERROR;
                        }
                        return ty;
                    }
                    return Ty::alias(traits::alias_of(*inherited, self_ty, name));
                }
                [] => errors.push((at, format!("associated type `{name}` not found for `Self`"))),
                _ => errors.push((at, format!("ambiguous associated type `{name}` for `Self`"))),
            }
            return Ty::ERROR;
        }
    }
    if !matches!(self_ty.kind(), TyKind::Param(_)) {
        errors.push((range, "ambiguous associated type".into()));
        return Ty::ERROR;
    }
    // `T::Out`: from the one trait among its bounds' with a type `Out`.
    let mut found: Vec<TraitRef> = Vec::new();
    for p in scope.bounds {
        if p.self_ty == self_ty
            && has_type(p.trait_ref.trait_)
            && !found.iter().any(|t| t.trait_ == p.trait_ref.trait_)
        {
            found.push(p.trait_ref);
        }
    }
    match found.as_slice() {
        [trait_ref] => Ty::alias(traits::alias_of(*trait_ref, self_ty, name)),
        [] => {
            errors.push((
                at,
                format!("associated type `{name}` not found for `{self_ty}`"),
            ));
            Ty::ERROR
        }
        _ => {
            errors.push((
                range,
                format!("ambiguous associated type `{name}` in bounds of `{self_ty}`"),
            ));
            Ty::ERROR
        }
    }
}

/// The bounds of `generics`, as [`traits::resolve_predicates`]; `T::Out`
/// in one may name the associated type of a bound on `T` in another, so
/// if any failed, they are resolved again with the others in scope.
fn resolve_predicates(
    items: &ItemTree,
    sigs: &Signatures,
    module: ModuleId,
    generics: &Generics,
    scope: TypeScope<'_>,
    errors: &mut Vec<(TextRange, String)>,
) -> traits::Bounds {
    let mut first = Vec::new();
    let resolved = traits::resolve_predicates(items, module, generics, scope, &mut first);
    if first.is_empty() {
        return resolved;
    }
    let all: Vec<Predicate> = scope
        .bounds
        .iter()
        .chain(&resolved.predicates)
        .copied()
        .collect();
    let bounds = traits::elaborate(sigs, &all);
    let scope = TypeScope {
        bounds: &bounds,
        ..scope
    };
    traits::resolve_predicates(items, module, generics, scope, errors)
}

/// Struct `strukt`, named by `path`, with the generic arguments written
/// after its last segment, which must be one for each of its
/// parameters.
pub fn struct_type(
    items: &ItemTree,
    module: ModuleId,
    strukt: StructId,
    path: &Path,
    scope: TypeScope<'_>,
    range: TextRange,
    errors: &mut Vec<(TextRange, String)>,
) -> Ty {
    let def = &items.structs[strukt];
    // `core::ffi::CStr` is the type of `c"..."`, which the compiler
    // knows.
    if items.lang_enums.get(&Symbol::intern("CStr")) == Some(&strukt) {
        return Ty::new(TyKind::CStr);
    }
    let last = path.segments.len().saturating_sub(1);
    let at = path.range(last, range);
    let want = def.generics.len();
    for args in path.args.iter().flatten() {
        for b in args.bindings.iter() {
            errors.push((b.range(), flux_hir::MISPLACED_BINDING.into()));
        }
    }
    let none;
    let written = match path.args(last) {
        Some(written) => written,
        None if want == 0 => return Ty::adt(strukt, def.name, &[]),
        // `S` for `struct S<T = i32>` is `S<i32>`.
        None if def.generics.params[0].default.is_some() => {
            none = flux_hir::GenericArgs {
                lifetimes: Box::new([]),
                types: Box::new([]),
                ranges: Box::new([]),
                bindings: Box::new([]),
                range: at,
                parenthesized: false,
            };
            &none
        }
        None => {
            let kind = if def.is_enum() { "enum" } else { "struct" };
            errors.push((at, format!("missing generics for {kind} `{}`", def.name)));
            return Ty::ERROR;
        }
    };
    let required = (def.generics.params.iter())
        .take_while(|p| p.default.is_none())
        .count();
    let got = written.types.len();
    if got > want || got < required {
        let kind = if def.is_enum() { "enum" } else { "struct" };
        let verb = if got == 1 { "was" } else { "were" };
        let message = if required < want && got > want {
            format!(
                "{kind} takes at most {} but {} {verb} supplied",
                generic_arguments(want),
                generic_arguments(got),
            )
        } else if required < want {
            format!(
                "{kind} takes at least {} but {} {verb} supplied",
                generic_arguments(required),
                generic_arguments(got),
            )
        } else {
            wrong_arg_count(kind, want, got)
        };
        errors.push((at, message));
        return Ty::ERROR;
    }
    let mut args =
        resolve_generic_args(items, module, &def.generics.params, written, scope, errors);
    if args.len() != got {
        let kind = if def.is_enum() { "enum" } else { "struct" };
        errors.push((at, wrong_arg_count(kind, want, args.len())));
        return Ty::ERROR;
    }
    // Those left out are their defaults, in terms of the ones before.
    if args.len() < want {
        let names = generic_names(items, None, &def.generics);
        let own = TypeScope {
            generics: &names,
            signature_of: Some("structs"),
            lens: scope.lens,
            defaults: scope.defaults,
            ..TypeScope::default()
        };
        while args.len() < want {
            let Some((ty, ty_range)) = &def.generics.params[args.len()].default else {
                return Ty::ERROR;
            };
            let mut ignored = Vec::new();
            let ty = resolve_type(items, def.loc.module, ty, own, *ty_range, &mut ignored);
            let ty = ty.subst(&args);
            args.push(ty);
        }
    }
    if args.contains(&Ty::ERROR) {
        return Ty::ERROR;
    }
    let params = &def.generics.params;
    if let Some((_, &arg)) = args
        .iter()
        .enumerate()
        .find(|&(i, &a)| !params[i].maybe_unsized && scope.is_unsized(items, a))
    {
        // At the path and its arguments, as rustc points.
        let at = match (path.ranges.first(), path.args(last)) {
            (Some(first), Some(args)) if first.start() <= args.range.end() => {
                TextRange::new(first.start(), args.range.end())
            }
            _ => range,
        };
        errors.push((at, unsized_error(arg)));
        return Ty::ERROR;
    }
    Ty::adt(strukt, def.name, &args)
}

/// rustc's E0277 for a value of unsized type `ty`.
pub fn unsized_error(ty: Ty) -> String {
    format!("the size for values of type `{ty}` cannot be known at compilation time")
}

thread_local! {
    /// The type aliases being expanded, innermost last: one met again
    /// is a cycle, reported where the alias is checked.
    static EXPANDING: std::cell::RefCell<Vec<flux_hir::TypeAliasId>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Type alias `alias`, named by `path`, with the generic arguments
/// written after its last segment: the type it stands for, with them
/// for its parameters. Errors in the alias itself are reported where it
/// is checked, not at each use.
pub fn alias_type(
    items: &ItemTree,
    module: ModuleId,
    alias: flux_hir::TypeAliasId,
    path: &Path,
    scope: TypeScope<'_>,
    range: TextRange,
    errors: &mut Vec<(TextRange, String)>,
) -> Ty {
    let def = &items.type_aliases[alias];
    let last = path.segments.len().saturating_sub(1);
    let at = path.range(last, range);
    let want = def.generics.len();
    for args in path.args.iter().flatten() {
        for b in args.bindings.iter() {
            errors.push((b.range(), flux_hir::MISPLACED_BINDING.into()));
        }
    }
    let args: Vec<Ty> = match path.args(last) {
        None if want == 0 => Vec::new(),
        None => {
            errors.push((
                at,
                format!("missing generics for type alias `{}`", def.name),
            ));
            return Ty::ERROR;
        }
        Some(written) => {
            let args =
                resolve_generic_args(items, module, &def.generics.params, written, scope, errors);
            if args.len() != want {
                errors.push((at, wrong_arg_count("type alias", want, args.len())));
                return Ty::ERROR;
            }
            args
        }
    };
    let ty = expand_alias(items, alias, scope.lens, &mut Vec::new());
    if args.contains(&Ty::ERROR) {
        return Ty::ERROR;
    }
    ty.subst(&args)
}

/// The type alias `alias` stands for, in terms of its parameters, with
/// the errors in it; an error if it is part of a cycle of aliases.
fn expand_alias(
    items: &ItemTree,
    alias: flux_hir::TypeAliasId,
    lens: Option<&ArrayLens<'_>>,
    errors: &mut Vec<(TextRange, String)>,
) -> Ty {
    let cycle = EXPANDING.with(|stack| {
        let mut stack = stack.borrow_mut();
        if stack.contains(&alias) {
            return true;
        }
        stack.push(alias);
        false
    });
    if cycle {
        return Ty::ERROR;
    }
    let def = &items.type_aliases[alias];
    let generics: Vec<Symbol> = def.generics.params.iter().map(|p| p.name).collect();
    let scope = TypeScope {
        generics: &generics,
        signature_of: Some("type aliases"),
        lens,
        ..TypeScope::default()
    };
    let ty = resolve_type(items, def.loc.module, &def.ty, scope, def.ty_range, errors);
    EXPANDING.with(|stack| stack.borrow_mut().pop());
    ty
}

/// Whether expanding type alias `alias` meets `alias` again.
fn alias_cycle(items: &ItemTree, alias: flux_hir::TypeAliasId) -> bool {
    let def = &items.type_aliases[alias];
    let mut found = false;
    visit_paths(&def.ty, &mut |path: &Path| {
        if !found
            && let Ok(Resolved::Def(Def::TypeAlias(next))) =
                items.resolve(def.loc.module, path, Ns::Types)
        {
            found = reaches_alias(items, next, alias, &mut Vec::new());
        }
    });
    found
}

/// Whether type alias `from` names `target`, through other aliases.
fn reaches_alias(
    items: &ItemTree,
    from: flux_hir::TypeAliasId,
    target: flux_hir::TypeAliasId,
    seen: &mut Vec<flux_hir::TypeAliasId>,
) -> bool {
    if from == target {
        return true;
    }
    if seen.contains(&from) {
        return false;
    }
    seen.push(from);
    let def = &items.type_aliases[from];
    let mut found = false;
    visit_paths(&def.ty, &mut |path: &Path| {
        if !found
            && let Ok(Resolved::Def(Def::TypeAlias(next))) =
                items.resolve(def.loc.module, path, Ns::Types)
        {
            found = reaches_alias(items, next, target, seen);
        }
    });
    found
}

/// Calls `f` on every path in `ty`, generic arguments included.
fn visit_paths(ty: &TypeRef, f: &mut impl FnMut(&Path)) {
    match ty {
        TypeRef::Path(path) => {
            f(path);
            for args in path.args.iter().flatten() {
                for arg in args.types.iter() {
                    visit_paths(arg, f);
                }
            }
            if let Some(qself) = &path.qself {
                visit_paths(&qself.ty, f);
            }
        }
        TypeRef::Tuple(elements) => elements.iter().for_each(|e| visit_paths(e, f)),
        TypeRef::Ptr { pointee, .. }
        | TypeRef::Ref { pointee, .. }
        | TypeRef::Array { elem: pointee, .. }
        | TypeRef::Slice(pointee) => visit_paths(pointee, f),
        TypeRef::FnPtr { params, ret, .. } => params
            .iter()
            .chain([&**ret])
            .for_each(|t| visit_paths(t, f)),
        TypeRef::Dyn(d) => {
            for b in d.bounds.iter() {
                for args in b.path.args.iter().flatten() {
                    args.types.iter().for_each(|t| visit_paths(t, f));
                    args.bindings.iter().for_each(|b| visit_paths(&b.ty, f));
                }
            }
        }
        TypeRef::Never
        | TypeRef::Infer
        | TypeRef::Const(_)
        | TypeRef::ImplTrait(_)
        | TypeRef::ImplParam(_)
        | TypeRef::Opaque(_)
        | TypeRef::Error => {}
    }
}

/// The primitive type called `name`.
pub fn primitive(name: Symbol) -> Result<Ty, String> {
    if name == sym::bool {
        return Ok(Ty::BOOL);
    }
    if name == sym::char {
        return Ok(Ty::CHAR);
    }
    if name == sym::str {
        return Ok(Ty::STR);
    }
    match IntTy::from_name(name) {
        Some(int) => Ok(Ty::int(int)),
        None => Err(format!("`{name}` is not supported yet")),
    }
}

/// The `impl` blocks' type for methods on `ty`, if it has any.
pub fn impl_target(items: &ItemTree, ty: Ty) -> Option<ImplTarget> {
    match ty.kind() {
        TyKind::Adt(adt, _) => Some(ImplTarget::Struct(adt.strukt)),
        // `core`'s `impl CStr` block.
        TyKind::CStr => items
            .lang_enums
            .get(&Symbol::intern("CStr"))
            .map(|&s| ImplTarget::Struct(s)),
        TyKind::Int(int) => Some(ImplTarget::Primitive(int.name())),
        TyKind::Bool => Some(ImplTarget::Primitive(sym::bool)),
        TyKind::Char => Some(ImplTarget::Primitive(sym::char)),
        TyKind::Str => Some(ImplTarget::Primitive(sym::str)),
        TyKind::Slice(_) => Some(ImplTarget::Primitive(flux_hir::slice_target())),
        TyKind::Array(..) => Some(ImplTarget::Primitive(flux_hir::array_target())),
        TyKind::Dynamic(d) => Some(ImplTarget::Object(d.trait_)),
        TyKind::RawPtr(m, _) => Some(ImplTarget::Primitive(flux_hir::raw_ptr_target(
            m == Mutability::Mut,
        ))),
        _ => None,
    }
}

/// What `ty` holds if it is `alloc`'s `Box<T>` (`#[lang = "owned_box"]`),
/// which dereferences as a built-in pointer that owns its value does, as
/// rustc's: moves out of `*b`, and `*b` is mutable where `b` is.
pub fn box_pointee(ty: Ty) -> Option<Ty> {
    static OWNED_BOX: std::sync::OnceLock<Option<StructId>> = std::sync::OnceLock::new();
    let TyKind::Adt(adt, args) = ty.kind() else {
        return None;
    };
    let owned_box = OWNED_BOX.get_or_init(|| {
        flux_hir::Core::get()
            .items
            .lang_enums
            .get(&Symbol::intern("owned_box"))
            .copied()
    });
    (Some(adt.strukt) == *owned_box)
        .then(|| args.first().copied())
        .flatten()
}

/// Whether `strukt` is `core`'s struct with `#[lang = "{lang}"]`.
pub fn is_lang_struct(items: &ItemTree, strukt: StructId, lang: &str) -> bool {
    items.lang_enums.get(&Symbol::intern(lang)) == Some(&strukt)
}

/// What `Self` means in an item in `owner`, if any: an `impl` block's
/// type, or a trait's first parameter.
pub fn self_ty(sigs: &Signatures, owner: Option<Owner>) -> Option<Ty> {
    match owner? {
        Owner::Impl(imp) => Some(sigs.impls.get(imp).copied().unwrap_or(Ty::ERROR)),
        Owner::Trait(_) => Some(Ty::param(0, kw::SelfUpper)),
    }
}

fn resolve_or_report(
    items: &ItemTree,
    module: ModuleId,
    ty: &TypeRef,
    scope: TypeScope<'_>,
    range: TextRange,
    diagnostics: &mut Vec<Diagnostic>,
) -> Ty {
    let mut errors = Vec::new();
    let ty = resolve_type(items, module, ty, scope, range, &mut errors);
    diagnostics.extend(
        errors
            .into_iter()
            .map(|(at, message)| Diagnostic::new(at, message)),
    );
    ty
}

/// Whether `ty` contains a reference. References are allowed only as
/// parameters and in local variables so far, where flux can check them
/// without lifetimes.
pub(crate) fn contains_ref(ty: Ty) -> bool {
    match ty.kind() {
        TyKind::Ref(..) => true,
        TyKind::RawPtr(_, pointee) | TyKind::Array(pointee, _) | TyKind::Slice(pointee) => {
            contains_ref(pointee)
        }
        TyKind::Tuple(elements) | TyKind::Adt(_, elements) => {
            elements.iter().any(|&e| contains_ref(e))
        }
        _ => false,
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
        TyKind::Alias(alias) => alias.args.iter().any(|&t| uses_param(t, index)),
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

/// The signature of every function and the field types of every struct
/// in `items`.
pub fn signatures(items: &ItemTree) -> (Signatures, Vec<Diagnostic>) {
    signatures_checked(items, false, None)
}

/// Like [`signatures`]; `check_core` checks `core`'s `impl` blocks
/// against their traits too, which its tests do instead of every build.
///
/// `core`'s signatures are resolved once per process; a program's are
/// resolved after them, unless `check_core` resolves everything again.
///
/// The lengths of array types named by constants (`[u8; N]`) are taken
/// from `lens`; those it does not know make the types errors, and are
/// noted in it as missing.
pub fn signatures_checked(
    items: &ItemTree,
    check_core: bool,
    lens: Option<&ArrayLens<'_>>,
) -> (Signatures, Vec<Diagnostic>) {
    let core = items.core;
    if check_core || core == ItemCounts::default() {
        let checked = if check_core { 0 } else { core.impls };
        // `core`'s lengths are known without evaluation, as where its
        // signatures are shared.
        let known = match lens {
            None if core != ItemCounts::default() => core_lens_values(),
            _ => LenValues::default(),
        };
        let arg_tys = ArgTys::default();
        let core_lens = ArrayLens::new(&known, &arg_tys);
        let (mut sigs, diagnostics) = extend_signatures(
            items,
            Signatures::default(),
            ItemCounts::default(),
            checked,
            lens.or(Some(&core_lens)),
        );
        if lens.is_none() {
            sigs.array_lens = known;
        }
        return (sigs, diagnostics);
    }
    let (core_sigs, core_diagnostics) = core_signatures();
    let sigs = core_sigs.fork_for_extension();
    let mut diagnostics = core_diagnostics.clone();
    let (sigs, more) = extend_signatures(items, sigs, core, core.impls, lens);
    diagnostics.extend(more);
    (sigs, diagnostics)
}

/// The signatures of [`Core`]'s items, its `impl` blocks unchecked as in
/// every build, resolved the first time they are asked for.
fn core_signatures() -> &'static (Signatures, Vec<Diagnostic>) {
    static CORE: OnceLock<(Signatures, Vec<Diagnostic>)> = OnceLock::new();
    CORE.get_or_init(|| {
        let items = &Core::get().items;
        let unchecked = items.impls.len() as u32;
        let known = core_lens_values();
        let arg_tys = ArgTys::default();
        let lens = ArrayLens::new(&known, &arg_tys);
        let (mut sigs, diagnostics) = extend_signatures(
            items,
            Signatures::default(),
            ItemCounts::default(),
            unchecked,
            Some(&lens),
        );
        debug_assert!(
            lens.missing.borrow().is_empty(),
            "core's array lengths name only constants whose values are literals"
        );
        sigs.array_lens = known;
        sigs.freeze();
        (sigs, diagnostics)
    })
}

/// Resolves the signatures of `items` from `from` on, given `sigs`, the
/// signatures of those before, which do not depend on them, and checks
/// them; the `impl` blocks from `checked_impls` on are checked against
/// their traits.
fn extend_signatures(
    items: &ItemTree,
    mut sigs: Signatures,
    from: ItemCounts,
    checked_impls: u32,
    lens: Option<&ArrayLens<'_>>,
) -> (Signatures, Vec<Diagnostic>) {
    sigs.traits_in_scope = in_scope::TraitsInScope::new(items);
    sigs.builtins = Builtins::of(items);
    let mut diagnostics = Vec::new();
    let mut error = |file, range, message: &str| {
        let mut d = Diagnostic::new(range, message);
        d.file = file;
        diagnostics.push(d);
    };
    let mut reported = Vec::new();
    fn resolve(
        items: &ItemTree,
        loc: ItemLoc,
        ty: &TypeRef,
        scope: TypeScope<'_>,
        range: TextRange,
        reported: &mut Vec<Diagnostic>,
    ) -> Ty {
        let start = reported.len();
        let ty = resolve_or_report(items, loc.module, ty, scope, range, reported);
        for d in &mut reported[start..] {
            d.file = loc.file;
        }
        ty
    }
    for (id, imp) in items.impls.iter().skip(from.impls as usize) {
        let ty = impl_type(items, imp, lens, &mut reported);
        sigs.impls.insert(id, ty);
    }
    sigs.impl_index = impl_index::ImplIndex::new(items, &sigs);
    let in_file = |errors: Vec<(TextRange, String)>, file, reported: &mut Vec<Diagnostic>| {
        reported.extend(errors.into_iter().map(|(range, message)| Diagnostic {
            file,
            range,
            message,
        }));
    };
    // Traits' defaults first: bounds anywhere may leave them out.
    let mut defaults = std::mem::take(&mut sigs.trait_defaults);
    for (id, t) in items.traits.iter().skip(from.traits as usize) {
        let generics = generic_names(items, Some(Owner::Trait(id)), &Generics::default());
        let scope = TypeScope {
            self_ty: Some(Ty::param(0, kw::SelfUpper)),
            generics: &generics,
            signature_of: Some("traits"),
            lens,
            ..TypeScope::default()
        };
        let mut errors = Vec::new();
        let resolved: Box<[Option<Ty>]> = t
            .generics
            .params
            .iter()
            .map(|p| {
                let (ty, range) = p.default.as_ref()?;
                Some(resolve_type(
                    items,
                    t.loc.module,
                    ty,
                    scope,
                    *range,
                    &mut errors,
                ))
            })
            .collect();
        in_file(errors, t.loc.file, &mut reported);
        defaults.insert(id, resolved);
    }
    sigs.trait_defaults = defaults;
    for (id, t) in items.traits.iter().skip(from.traits as usize) {
        let generics = generic_names(items, Some(Owner::Trait(id)), &Generics::default());
        let consts = generic_consts(items, Some(Owner::Trait(id)), &Generics::default());
        let self_param = Ty::param(0, kw::SelfUpper);
        let bounds = [traits::trait_self_predicate(items, id)];
        let mut errors = Vec::new();
        check_const_params(items, t.loc.module, &t.generics, &generics, &mut errors);
        in_file(errors, t.loc.file, &mut reported);
        let scope = TypeScope {
            self_ty: Some(self_param),
            generics: &generics,
            consts: &consts,
            signature_of: Some("traits"),
            bounds: &bounds,
            defaults: Some(&sigs.trait_defaults),
            lens,
            ..TypeScope::default()
        };
        let mut errors = Vec::new();
        let mut supertraits = Vec::new();
        let mut projections = Vec::new();
        let mut implied = Vec::new();
        for b in t.supertraits.iter() {
            if let Some((trait_ref, bindings)) =
                traits::resolve_bound(items, t.loc.module, b, self_param, scope, &mut errors)
            {
                supertraits.push(trait_ref);
                implied.extend(bindings.iter().copied());
                projections.extend(bindings);
            }
        }
        let mut predicates = Vec::new();
        let resolved =
            resolve_predicates(items, &sigs, t.loc.module, &t.generics, scope, &mut errors);
        // `where Self: Sup` is a supertrait too.
        for p in resolved.predicates {
            if p.self_ty == self_param {
                supertraits.push(p.trait_ref);
            } else {
                predicates.push(p);
            }
        }
        implied.extend(
            resolved
                .projections
                .iter()
                .filter(|p| p.alias.args[0] == self_param),
        );
        projections.extend(resolved.projections);
        let mut type_bounds = Vec::new();
        for ty in &t.types {
            let alias = Ty::alias(traits::alias_of(bounds[0].trait_ref, self_param, ty.name));
            for b in ty.bounds.iter() {
                if let Some((trait_ref, bindings)) =
                    traits::resolve_bound(items, t.loc.module, b, alias, scope, &mut errors)
                {
                    type_bounds.push((ty.name, trait_ref));
                    implied.extend(bindings.iter().copied());
                    projections.extend(bindings);
                }
            }
        }
        in_file(errors, t.loc.file, &mut reported);
        let defaults = sigs.trait_defaults.get(id).cloned().unwrap_or_default();
        sigs.traits.insert(
            id,
            TraitSig {
                supertraits: supertraits.into(),
                defaults,
                type_bounds: type_bounds.into(),
                implied: implied.into(),
            },
        );
        sigs.trait_predicates.insert(id, predicates.into());
        if !projections.is_empty() {
            sigs.trait_projections.insert(id, projections.into());
        }
    }
    // Structs' bounds, then their fields, in which `T::Out` may name
    // the associated type of a bound on `T`.
    for (id, s) in items.structs.iter().skip(from.structs as usize) {
        if s.is_union {
            sigs.unions.insert(id);
        }
        let generics: Vec<Symbol> = s.generics.params.iter().map(|p| p.name).collect();
        let consts: Vec<Option<Ty>> = s.generics.params.iter().map(param_const_ty).collect();
        let unsized_params = unsized_params(items, None, &s.generics);
        let scope = TypeScope {
            self_ty: Some(Ty::adt(id, s.name, &params_of(&s.generics, 0))),
            generics: &generics,
            consts: &consts,
            unsized_params: &unsized_params,
            signature_of: Some("structs"),
            defaults: Some(&sigs.trait_defaults),
            lens,
            ..TypeScope::default()
        };
        let mut errors = Vec::new();
        check_const_params(items, s.loc.module, &s.generics, &generics, &mut errors);
        let own = resolve_predicates(items, &sigs, s.loc.module, &s.generics, scope, &mut errors);
        in_file(errors, s.loc.file, &mut reported);
        let bounds = traits::elaborate(&sigs, &own.predicates);
        let scope = TypeScope {
            bounds: &bounds,
            ..scope
        };
        let mut starts = Vec::new();
        let mut fields: Vec<Ty> = Vec::new();
        for v in 0..s.variant_count() as u32 {
            starts.push(fields.len() as u32);
            let variant = s.variant(v).fields;
            fields.extend(variant.iter().map(|f| {
                let ty = resolve(items, s.loc, &f.ty, scope, f.range, &mut reported);
                if scope.is_unsized(items, ty) {
                    let mut d = Diagnostic::new(f.range, "unsized fields are not supported yet");
                    d.file = s.loc.file;
                    reported.push(d);
                    return Ty::ERROR;
                }
                ty
            }));
        }
        starts.push(fields.len() as u32);
        sigs.structs.insert(id, Ty::list(&fields));
        if s.is_enum() {
            sigs.variant_starts.insert(id, starts.into());
            sigs.discriminants.insert(id, discriminant_ty(items, id));
        }
        if !own.predicates.is_empty() {
            sigs.struct_predicates.insert(id, own.predicates.into());
        }
        if !own.projections.is_empty() {
            sigs.struct_projections.insert(id, own.projections.into());
        }
    }
    for (id, imp) in items.impls.iter().skip(from.impls as usize) {
        if imp.trait_ref.is_none() && imp.generics.predicates.is_empty() {
            continue;
        }
        let generics = generic_names(items, Some(Owner::Impl(id)), &Generics::default());
        let consts = generic_consts(items, Some(Owner::Impl(id)), &Generics::default());
        let unsized_params = unsized_params(items, Some(Owner::Impl(id)), &Generics::default());
        let self_ty = sigs.impls[id];
        let scope = TypeScope {
            self_ty: Some(self_ty),
            generics: &generics,
            consts: &consts,
            unsized_params: &unsized_params,
            signature_of: Some("implementations"),
            defaults: Some(&sigs.trait_defaults),
            lens,
            ..TypeScope::default()
        };
        let mut errors = Vec::new();
        let mut self_trait = None;
        let mut projections = Vec::new();
        if let (Some(t), Some(bound)) = (imp.trait_, &imp.trait_ref)
            && let Some((trait_ref, bindings)) =
                traits::trait_ref(items, imp.module, t, bound, self_ty, scope, &mut errors)
        {
            sigs.impl_traits.insert(id, trait_ref);
            self_trait = Some(trait_ref);
            // `impl Tr<Out = i32> for S` says nothing: its block does.
            if !bindings.is_empty() {
                for args in bound.path.args.iter().flatten() {
                    for b in args.bindings.iter() {
                        errors.push((b.range(), flux_hir::MISPLACED_BINDING.into()));
                    }
                }
            }
        }
        let resolved =
            resolve_predicates(items, &sigs, imp.module, &imp.generics, scope, &mut errors);
        projections.extend(resolved.projections);
        let bounds = traits::elaborate(&sigs, &resolved.predicates);
        let self_supertraits = traits::associated_self_bounds(&sigs, self_trait, Some(self_ty));
        let self_inherited_types = traits::associated_self_types(items, &sigs, &self_supertraits);
        let scope = TypeScope {
            bounds: &bounds,
            self_trait,
            self_supertraits: &self_supertraits,
            self_inherited_types: &self_inherited_types,
            ..scope
        };
        let types: Box<[(Symbol, Ty)]> = imp
            .types
            .iter()
            .map(|t| {
                let ty = resolve_type(items, imp.module, &t.ty, scope, t.ty_range, &mut errors);
                (t.name, ty)
            })
            .collect();
        in_file(errors, imp.file, &mut reported);
        if !types.is_empty() {
            sigs.impl_types.insert(id, types);
        }
        if !resolved.predicates.is_empty() {
            sigs.impl_predicates.insert(id, resolved.predicates.into());
        }
        if !projections.is_empty() {
            sigs.impl_projections.insert(id, projections.into());
        }
    }
    for (id, konst) in items.consts.iter().skip(from.consts as usize) {
        // A discriminant has the enum's `repr` type.
        if let Some((strukt, _)) = konst.discriminant {
            sigs.consts
                .insert(id, Ty::int(discriminant_ty(items, strukt)));
            continue;
        }
        // A const generic argument has its parameter's type, known once
        // the path it is in is resolved.
        if konst.anon.is_some_and(|a| a.is_arg) {
            let ty = lens.and_then(|l| l.arg_ty(id)).unwrap_or(Ty::ERROR);
            sigs.consts.insert(id, ty);
            continue;
        }
        let generics = generic_names(items, konst.owner, &Generics::default());
        // Its owner's bounds, for `Self::A` or `T::A` in its type.
        let bounds = traits::elaborate(&sigs, &traits::owner_predicates(items, &sigs, konst.owner));
        let self_trait = konst
            .owner
            .and_then(Owner::as_impl)
            .and_then(|imp| sigs.impl_traits.get(imp).copied());
        let self_supertraits =
            traits::associated_self_bounds(&sigs, self_trait, self_ty(&sigs, konst.owner));
        let self_inherited_types = traits::associated_self_types(items, &sigs, &self_supertraits);
        let scope = TypeScope {
            self_ty: self_ty(&sigs, konst.owner),
            self_supertraits: &self_supertraits,
            self_inherited_types: &self_inherited_types,
            generics: &generics,
            consts: &[],
            signature_of: Some(match (konst.owner, konst.is_static) {
                (Some(_), _) => "associated constants",
                (None, Some(_)) => "static variables",
                (None, None) => "constants",
            }),
            bounds: &bounds,
            self_trait: konst
                .owner
                .and_then(Owner::as_impl)
                .and_then(|imp| sigs.impl_traits.get(imp).copied()),
            self_types: &[],
            defaults: Some(&sigs.trait_defaults),
            lens,
            function: None,
            unsized_params: &[],
        };
        let mut ty = resolve(
            items,
            konst.loc,
            &konst.ty,
            scope,
            konst.ty_range,
            &mut reported,
        );
        if ty.is_unsized() {
            let mut d = Diagnostic::new(konst.ty_range, unsized_error(ty));
            d.file = konst.loc.file;
            reported.push(d);
            ty = Ty::ERROR;
        }
        if let Some(mutable) = konst.is_static {
            let mut error = |message: String| {
                reported.push(Diagnostic {
                    file: konst.loc.file,
                    range: konst.ty_range,
                    message,
                });
            };
            fn mutable_ref(ty: Ty) -> bool {
                matches!(ty.kind(), TyKind::Ref(Mutability::Mut, _))
                    || ty.children().any(mutable_ref)
            }
            if mutable_ref(ty) {
                error("mutable references in statics are not supported yet".to_owned());
            } else if !mutable && let Some(pointer) = not_sync(items, &sigs, ty, &mut Vec::new()) {
                // Name the innermost failing type, including a trait
                // object's lifetime bound as rustc does.
                let name = if pointer.is_dyn() {
                    format!("({pointer} + 'static)")
                } else {
                    pointer.to_string()
                };
                error(format!("`{name}` cannot be shared between threads safely"));
            }
        }
        sigs.consts.insert(id, ty);
    }
    for (id, function) in items.functions.iter().skip(from.functions as usize) {
        let generics = generic_names(items, function.owner, &function.generics);
        let consts = generic_consts(items, function.owner, &function.generics);
        let unsized_params = unsized_params(items, function.owner, &function.generics);
        let mut errors = Vec::new();
        check_const_params(
            items,
            function.loc.module,
            &function.generics,
            &generics,
            &mut errors,
        );
        in_file(errors, function.loc.file, &mut reported);
        let self_trait = items
            .impl_of(id)
            .and_then(|imp| sigs.impl_traits.get(imp).copied());
        let owner_bounds = traits::owner_predicates(items, &sigs, function.owner);
        let self_types: Vec<(Symbol, Ty)> = match items.impl_of(id) {
            Some(imp) if self_trait.is_some() => {
                sigs.impl_types.get(imp).map_or(Vec::new(), |t| t.to_vec())
            }
            _ => Vec::new(),
        };
        let self_supertraits =
            traits::associated_self_bounds(&sigs, self_trait, self_ty(&sigs, function.owner));
        let self_inherited_types = traits::associated_self_types(items, &sigs, &self_supertraits);
        let scope = TypeScope {
            self_ty: self_ty(&sigs, function.owner),
            self_supertraits: &self_supertraits,
            self_inherited_types: &self_inherited_types,
            generics: &generics,
            consts: &consts,
            unsized_params: &unsized_params,
            signature_of: Some("functions"),
            self_trait,
            self_types: &self_types,
            defaults: Some(&sigs.trait_defaults),
            lens,
            ..TypeScope::default()
        };
        // Its bounds first, then its types, in which `T::Out` may name
        // their associated types.
        let mut own = traits::Bounds::default();
        if !function.generics.predicates.is_empty() {
            let elaborated = traits::elaborate(&sigs, &owner_bounds);
            let scope = TypeScope {
                bounds: &elaborated,
                ..scope
            };
            let mut errors = Vec::new();
            own = resolve_predicates(
                items,
                &sigs,
                function.loc.module,
                &function.generics,
                scope,
                &mut errors,
            );
            in_file(errors, function.loc.file, &mut reported);
        }
        let all: Vec<Predicate> = owner_bounds
            .iter()
            .chain(&own.predicates)
            .copied()
            .collect();
        let bounds = traits::elaborate(&sigs, &all);
        let scope = TypeScope {
            bounds: &bounds,
            ..scope
        };
        // Parameters and return values must be sized; a declaration
        // without a body may name a trait's `Self` or a `?Sized`
        // parameter, as rustc checks only bodies' (and not where a bound
        // makes the type `Sized`).
        let has_body = function.has_body;
        let sized = |ty: Ty, range: TextRange, reported: &mut Vec<Diagnostic>| {
            // A parameter keeps its type, which the body's bindings have.
            if matches!(ty.kind(), TyKind::Param(_)) {
                if has_body && scope.is_unsized(items, ty) {
                    let mut d = Diagnostic::new(range, unsized_error(ty));
                    d.file = function.loc.file;
                    reported.push(d);
                }
                return ty;
            }
            if ty.is_unsized() {
                // A trait object here has its default bound, `'static`,
                // which rustc names.
                let message = if ty.is_dyn() {
                    format!(
                        "the size for values of type `({ty} + 'static)` cannot be known at compilation time"
                    )
                } else {
                    unsized_error(ty)
                };
                let mut d = Diagnostic::new(range, message);
                d.file = function.loc.file;
                reported.push(d);
                return Ty::ERROR;
            }
            ty
        };
        let params = function
            .params
            .iter()
            .map(|p| {
                let ty = resolve(items, function.loc, &p.ty, scope, p.range, &mut reported);
                sized(ty, p.range, &mut reported)
            })
            .collect();
        let ret = resolve(
            items,
            function.loc,
            &function.ret,
            TypeScope {
                function: Some(id),
                ..scope
            },
            function.ret_range,
            &mut reported,
        );
        let ret = sized(ret, function.ret_range, &mut reported);
        sigs.fns.insert(id, FnSig { params, ret });
        if !function.opaques.is_empty() {
            let mut errors = Vec::new();
            let opaques = opaque_sigs(items, function, scope, &mut errors);
            in_file(errors, function.loc.file, &mut reported);
            sigs.opaques.insert(id, opaques);
        }
        if !own.predicates.is_empty() {
            sigs.fn_predicates.insert(id, own.predicates.into());
        }
        if !own.projections.is_empty() {
            sigs.fn_projections.insert(id, own.projections.into());
        }
    }
    for (id, s) in items.structs.iter().skip(from.structs as usize) {
        // A parameter must be a field's type, of some variant for an
        // enum, except `PhantomData`'s, which acts as if it were one.
        if is_lang_struct(items, id, "phantom_data") {
            continue;
        }
        for (i, param) in s.generics.params.iter().enumerate() {
            let fields = sigs.structs[id];
            // Const parameters need not be used, as in rustc.
            if param.is_const() {
                continue;
            }
            if !fields.iter().any(|&f| uses_param(f, i as u32)) && !fields.contains(&Ty::ERROR) {
                error(
                    s.loc.file,
                    param.range,
                    &format!("type parameter `{}` is never used", param.name),
                );
            }
        }
    }
    for (id, alias) in items.type_aliases.iter().skip(from.type_aliases as usize) {
        check_alias(items, id, alias, lens, &mut reported);
    }
    regions::resolve_regions(items, &mut sigs, from, &mut reported);
    traits::check_well_formed(items, &sigs, from, &mut reported);
    duplicate_assoc_items(items, &sigs, &mut reported, from.impls as usize);
    traits::check_impls(items, &sigs, &mut reported, checked_impls);
    // Union fields are checked after all signatures and impl predicates exist:
    // a concrete generic struct's Copy impl may depend on its arguments.
    for (id, data) in items.structs.iter().skip(from.structs as usize) {
        if !data.is_union {
            continue;
        }
        for (field, &ty) in data.fields.iter().zip(sigs.structs[id]) {
            if ty.has_error() {
                continue;
            }
            let ty = traits::normalize(items, &sigs, &[], ty);
            let message = if !supported_union_field(items, &sigs, ty, &mut FxHashMap::default(), 0)
            {
                Some("this union field type is not supported yet")
            } else if !traits::is_copy(items, &sigs, &[], ty)
                || traits::needs_drop(items, &sigs, &[], ty)
            {
                Some("union fields must implement Copy")
            } else {
                None
            };
            if let Some(message) = message {
                let mut d = Diagnostic::new(field.range, message);
                d.file = data.loc.file;
                reported.push(d);
            }
        }
    }
    sigs.cyclic |= traits::check_supertrait_cycles(items, &sigs, &mut reported, from);
    traits::check_trivial_bounds(items, &sigs, &mut reported, from);
    for (id, imp) in items.impls.iter().skip(from.impls as usize) {
        let ty = sigs.impls[id];
        let trait_args = sigs.impl_traits.get(id).map_or(&[][..], |t| t.args);
        // As in rustc, a parameter an associated type of constrained ones
        // is bound to is constrained too.
        let count = imp.generics.params.len();
        let mut constrained: Vec<bool> = (0..count as u32)
            .map(|i| mentions_param(ty, i) || trait_args.iter().any(|&a| mentions_param(a, i)))
            .collect();
        loop {
            let mut progress = false;
            for p in sigs.impl_projections.get(id).into_iter().flatten() {
                let inputs = (0..count as u32)
                    .filter(|&i| p.alias.args.iter().any(|&a| mentions_param(a, i)))
                    .all(|i| constrained[i as usize]);
                if !inputs {
                    continue;
                }
                for (i, c) in constrained.iter_mut().enumerate() {
                    if !*c && mentions_param(p.ty, i as u32) {
                        *c = true;
                        progress = true;
                    }
                }
            }
            if !progress {
                break;
            }
        }
        for (i, param) in imp.generics.params.iter().enumerate() {
            let constrained = constrained[i];
            if ty != Ty::ERROR && !constrained {
                error(
                    imp.file,
                    param.range,
                    &format!(
                        "the type parameter `{}` is not constrained by the impl trait, self type, or predicates",
                        param.name
                    ),
                );
            }
        }
    }
    for (id, s) in items.structs.iter().skip(from.structs as usize) {
        let ty = Ty::adt(id, s.name, &params_of(&s.generics, 0));
        if contains(&sigs, ty, id, &mut Vec::new()) {
            error(
                s.loc.file,
                s.def_range,
                &format!("recursive type `{}` has infinite size", s.name),
            );
        }
    }
    for (id, function) in items.functions.iter().skip(from.functions as usize) {
        let sig = &sigs.fns[id];
        let params = function.params.iter().zip(&sig.params);
        if function.is_extern {
            for (param, &ty) in params.clone() {
                if contains_ref(ty) {
                    error(
                        function.loc.file,
                        param.range,
                        "passing references to C is not supported yet",
                    );
                }
            }
            let types = params
                .map(|(p, &ty)| (ty, p.range))
                .chain([(sig.ret, function.ret_range)]);
            for (ty, range) in types {
                let sequence = match ty.kind() {
                    TyKind::Array(..) => true,
                    TyKind::Ref(_, t) | TyKind::RawPtr(_, t) => t.is_unsized(),
                    _ => false,
                };
                if sequence {
                    error(
                        function.loc.file,
                        range,
                        "passing arrays and slices to C is not supported yet",
                    );
                } else if !matches!(ty.kind(), TyKind::Tuple([])) && sigs.contents(ty).is_some() {
                    error(
                        function.loc.file,
                        range,
                        "passing structs and tuples to C is not supported yet",
                    );
                }
            }
        }
    }
    diagnostics.extend(reported);
    (sigs, diagnostics)
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
    let ty = expand_alias(items, id, lens, &mut errors);
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

/// The `drop` methods of `core`'s `Drop` impls (`array::IntoIter`'s),
/// which no call names but drop glue calls.
pub fn core_drop_fns(items: &ItemTree) -> Vec<FunctionId> {
    let Some(&t) = items.lang_traits.get(&sym::drop) else {
        return Vec::new();
    };
    let impls = items.trait_impls.get(&t).into_iter().flatten();
    impls
        .filter(|&&imp| items.is_core(items.impls[imp].module))
        .flat_map(|&imp| items.impls[imp].functions.iter().copied())
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
            .filter_map(|(_, ty)| match ty.kind() {
                TyKind::Ref(_, p) | TyKind::RawPtr(_, p) => match p.kind() {
                    TyKind::Dynamic(d) => Some(d.trait_),
                    _ => None,
                },
                _ => None,
            })
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
    Diagnostic::sort(&mut diagnostics);
    TypedFile {
        sigs,
        bodies,
        consts,
        diagnostics,
    }
}

/// The checks of the patterns of the bodies whose patterns name
/// constants ([`InferenceResult::deferred_pats`]), given `value`, the
/// value of a constant of an integer type or `bool` (`1` for `true`),
/// or `None` if it has none (reported). Bodies that name one without a
/// value are not checked. Also returns the functions with errors, which
/// rustc does not borrow-check.
pub fn check_deferred_patterns(
    hir: &Hir,
    typed: &TypedFile,
    value: &mut dyn FnMut(ConstInstance) -> Option<i128>,
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
                let ty = expand_alias(items, id, Some(&lens), &mut Vec::new());
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
        TyKind::Int(_) | TyKind::Bool => true,
        TyKind::Tuple([]) => true,
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
