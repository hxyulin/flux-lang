//! The type representation that the trait solver and the type checker
//! share: interned types ([`Ty`]), predicates, and the resolved
//! signatures of a program's items ([`Signatures`]), with the region
//! tables the borrow checker reads beside them.
//!
//! Everything here is data and pure functions over it. Resolving types
//! from the item tree is `flux-ty`'s, and trait solving is `flux-solve`'s.

pub mod flib;
pub mod fn_regions;
pub mod impl_index;
pub mod in_scope;
pub mod metadata;
pub mod query_identity;
pub mod regions;
pub mod traits;
pub mod ty;
pub mod type_identity;

use std::ops::Index;

use flux_arena::LayeredArenaMap;
use flux_hir::{
    Const, ConstId, Function, FunctionId, Generics, Impl, ItemTree, Struct, StructId, Trait,
};
use flux_intern::Symbol;

pub use metadata::Metadata;
pub use traits::{Predicate, Projection, TraitRef, TraitSig};
pub use ty::{
    AdtId, AliasTy, ClosureArgs, ClosureId, ClosureKind, ClosureOwner, DynTy, FloatTy, FnDefTy,
    FnProjection, FnPtrRegions, FnRegion, FnSigTy, GenericArgs, InferVar, IntTy, Mutability,
    OpaqueTy, ParamTy, Ty, TyKind, subst_list, symbol_list,
};

/// The types of the anonymous constants of const generic arguments.
pub type ArgTys = rustc_hash::FxHashMap<ConstId, Ty>;

/// The evaluated values of constants that array lengths and const
/// generic arguments name, as bits (`1` for `true`).
pub type LenValues = rustc_hash::FxHashMap<ConstId, Option<u128>>;

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
    /// Shared by constant evaluation and monomorphization of this graph.
    pub type_ids: type_identity::TypeIds,
    pub sized_dispatch: rustc_hash::FxHashMap<FunctionId, flux_hir::SizedDispatch>,
    pub unions: rustc_hash::FxHashSet<StructId>,
    pub fns: LayeredArenaMap<Function, FnSig>,
    /// The types of each struct's fields, and of each enum's, every
    /// variant's in turn: all that a value of it may contain.
    pub structs: LayeredArenaMap<Struct, &'static [Ty]>,
    /// Where each variant of an enum starts in its `structs` entry, and
    /// where the last ends.
    pub variant_starts: LayeredArenaMap<Struct, Box<[u32]>>,
    /// The type of each enum's discriminant: `flux_ty::discriminant_ty`.
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
    /// its return type), by index: `flux_solve::traits::opaque_bounds`.
    pub opaques: LayeredArenaMap<Function, Box<[OpaqueSig]>>,
    /// The hidden type of each opaque type, in terms of its function's
    /// generic parameters, once the function's body is inferred: what
    /// code generation sees (`flux_solve::traits::reveal`).
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
    /// Written bound index for each directly resolved predicate.
    pub bound_origins: Box<[u32]>,
    pub predicates: Box<[Predicate]>,
    pub projections: Box<[Projection]>,
}

/// The traits of `core` that `flux_solve::field_types` needs to know,
/// recorded by lang item so projection normalization does not guess by name.
#[derive(Debug, Clone, Default)]
pub struct Builtins {
    pub sized: Option<flux_hir::TraitId>,
    /// Traits that can have compiler-provided structural implementations.
    pub auto_traits: Vec<flux_hir::TraitId>,
}

impl Builtins {
    pub fn of(items: &ItemTree) -> Builtins {
        Builtins {
            sized: items.lang_traits.get(&flux_intern::sym::sized).copied(),
            auto_traits: ["send", "sync", "unpin", "unwind_safe", "ref_unwind_safe"]
                .into_iter()
                .filter_map(|name| items.lang_traits.get(&Symbol::intern(name)).copied())
                .collect(),
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
    pub fn fork_for_extension(&self) -> Self {
        Self {
            type_ids: self.type_ids.fork(),
            sized_dispatch: self.sized_dispatch.clone(),
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
            builtins: self.builtins.clone(),
        }
    }

    /// Freeze the library's maps once; copies retain a shared base and
    /// clone individual inherited values only when they are changed.
    pub fn freeze(&mut self) {
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
}

/// One more than the largest index of a generic parameter in `ty`, or 0.
pub fn max_param(ty: Ty) -> u32 {
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

/// Whether two `impl` blocks' types, each in terms of its own
/// parameters, may be the same type.
pub fn overlap(a: Ty, b: Ty) -> bool {
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
                && x.abi == y.abi
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
pub fn mentions_param(ty: Ty, index: u32) -> bool {
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
        TyKind::Dynamic(object) => object
            .args
            .iter()
            .chain(object.assoc_tys.iter())
            .any(|&t| mentions_param(t, index)),
        _ => false,
    }
}

pub fn params_of(generics: &Generics, first: usize) -> Vec<Ty> {
    generics
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| Ty::param((first + i) as u32, p.name))
        .collect()
}
