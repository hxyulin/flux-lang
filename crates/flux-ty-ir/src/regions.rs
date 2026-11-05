//! The lifetimes of signatures, beside their types, for the borrow
//! checker: each type's *region slots* ([`slots`]) and what an instance
//! of a signature has in them ([`instantiate`]). `flux-ty` resolves them
//! from the item tree.

use flux_hir::{Const, Function, Impl, ItemTree, Struct, Trait};
use flux_intern::Symbol;
use flux_syntax::TextRange;

use crate::{ClosureArgs, Ty, TyKind};

/// A lifetime in a signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Region {
    Static,
    /// The item's lifetime parameter with this index: its owner's
    /// first, then its own, then those its elided lifetimes stand for.
    Param(u32),
    /// Elided in a body, or erroneous (reported): the borrow checker
    /// infers it.
    Erased,
    /// A lifetime quantified by the enclosing bound. Non-callable predicates
    /// number their binder parameters independently of owner lifetimes.
    /// In a `Fn` bound, elided input lifetimes use input-tuple slots, so the
    /// result can retain the same input region at each fresh call.
    Late(u32),
}

/// Written type arguments' region slots, indexed in a call's full generic list.
/// Entries with entirely inferred slots need no additional call constraint.
pub type TypeArgRegions = Vec<(u32, Box<[Region]>)>;

/// The [`TypeArgRegions`] index of the type written before an inherent
/// associated function, `S<'a>` in `S::<'a>::f`: the slots of the callee's
/// `impl` block's self type, not of one of its generic arguments.
pub const IMPL_SELF_ARG: u32 = u32::MAX;

/// How a type's subtyping depends on one of its parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Variance {
    /// Not used: anything goes (and an error for a struct's parameter).
    Bivariant,
    Covariant,
    Contravariant,
    Invariant,
}

impl Variance {
    /// The variance of a use with variance `inner` inside a position
    /// with variance `self`.
    pub fn xform(self, inner: Variance) -> Variance {
        match (self, inner) {
            (Variance::Covariant, v) => v,
            (Variance::Contravariant, v) => v.flip(),
            (Variance::Invariant, Variance::Bivariant) => Variance::Bivariant,
            (Variance::Invariant, _) => Variance::Invariant,
            (Variance::Bivariant, _) => Variance::Bivariant,
        }
    }

    pub fn flip(self) -> Variance {
        match self {
            Variance::Covariant => Variance::Contravariant,
            Variance::Contravariant => Variance::Covariant,
            v => v,
        }
    }

    /// The variance of a parameter used both ways.
    pub fn join(self, other: Variance) -> Variance {
        match (self, other) {
            (Variance::Bivariant, v) | (v, Variance::Bivariant) => v,
            (a, b) if a == b => a,
            _ => Variance::Invariant,
        }
    }
}

/// `'a: 'b`, or `T: 'a`: what one lifetime or type outlives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outlives<R> {
    Region(R, R),
    /// A type without slots of its own (a parameter or an associated
    /// type) outlives a lifetime.
    Type(Ty, R),
}

/// A lifetime parameter's name, `None` for one an elided lifetime
/// stands for, and where it is declared (or elided).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifetimeName {
    pub name: Option<Symbol>,
    pub range: TextRange,
}

/// Supported provenance boundary for function-item callback inputs.
/// This is a signature-level classification, not Rust's complete early/late
/// parameter partition or a proof of higher-ranked universality.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CallbackBinder {
    /// Missing or unrepresented source/table mappings must fail closed.
    #[default]
    Unclassified,
    /// No signature loan is fixed by an own predicate lifetime. Type-only
    /// conditions and predicate-only lifetimes still require their original
    /// obligation proofs. Owner lifetimes have a separate conservative boundary.
    Independent,
    /// A signature lifetime also occurs in a predicate, including syntactically
    /// present alias arguments erased by normalization. Newly bound inputs are unsupported;
    /// original Param/Alias/IteratorItem inputs remain fixed.
    SignatureBound,
}

/// The lifetimes of a function's signature.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FnRegions {
    /// Its lifetime parameters, its owner's first.
    pub names: Box<[LifetimeName]>,
    /// How many of `names` are its owner's.
    pub owner: u32,
    pub params: Box<[Box<[Region]>]>,
    pub ret: Box<[Region]>,
    /// Projection input occurrences in function signatures,
    /// one list per parameter followed by the return type. Their anonymous
    /// lifetimes share this signature's names, but add no stored value slots.
    /// Trait-impl comparison retains its qualified-only legacy input policy.
    pub selected_inputs: Box<[Box<[ProjectionSelection]>]>,
    /// What its lifetimes outlive: its bounds and its owner's, and what
    /// its parameter and return types imply (`&'a T` that `T: 'a`).
    /// The body may assume them, and a call must meet them.
    pub outlives: Box<[Outlives<Region>]>,
    /// Explicit lifetime/type-outlives predicates on this signature.
    /// This does not include trait predicates; use `callback_binder` for the
    /// supported callback provenance boundary.
    pub has_bounds: bool,
    pub callback_binder: CallbackBinder,
    /// The slots of the types its own bounds are on (`&'a T` in `where
    /// &'a T: Tr`), as [`crate::Signatures::fn_predicates`] lists them.
    pub bound_selfs: Box<[Box<[Region]>]>,
    /// Each resolved predicate's trait arguments, in the same order.
    pub bound_traits: Box<[TraitRefRegions]>,
    /// The slots of what its own bounds say associated types are
    /// (`I: Iterator<Item = &'a T>`), as [`crate::Signatures::fn_projections`]
    /// lists them.
    pub projections: Box<[Box<[Region]>]>,
    /// Its opaque types', by index.
    pub opaques: Box<[OpaqueRegions]>,
}

/// The lifetimes of an opaque type (an `impl Trait` in a function's
/// return type), in terms of its function's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpaqueRegions {
    /// Direct declaration bounds, preserving their written origin and erased
    /// predicate so malformed or reordered tables cannot select another row.
    pub bounds: Box<[OpaqueBoundRegions]>,
    /// Those it captures, which are its region slots: all of its function's
    /// in edition 2024, those mentioned in older-edition bounds, or an
    /// explicit `use<..>` list.
    pub captures: Box<[Region]>,
    /// `'a` in `impl Tr + 'a`: what its hidden type outlives.
    pub outlives: Box<[Region]>,
    /// The slots of what its bounds say associated types are
    /// (`impl Iterator<Item = &'a T>`), as [`crate::OpaqueSig`] lists
    /// them; none where they cannot be matched, which only errors cause.
    pub projections: Box<[Box<[Region]>]>,
}

/// One written opaque bound and its separate trait argument lifetimes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpaqueBoundRegions {
    pub origin: u32,
    pub predicate: crate::Predicate,
    pub regions: TraitRefRegions,
}

/// The lifetimes of a struct's or enum's fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StructRegions {
    /// Concrete field selection inputs remain invariant and well formed even
    /// when the selected output no longer mentions all of their lifetimes.
    pub selected_inputs: Box<[(crate::AliasTy, Box<[Region]>)]>,
    /// Original declaration predicates, beside `Signatures::struct_predicates`.
    pub bound_selfs: Box<[Box<[Region]>]>,
    pub bound_traits: Box<[TraitRefRegions]>,
    /// Original associated equalities, beside `struct_projections`.
    pub projections: Box<[Box<[Region]>]>,
    /// How many lifetime parameters it has.
    pub lifetimes: u32,
    /// The slots of each field's type, as [`crate::Signatures::structs`] lists
    /// them.
    pub fields: Box<[Box<[Region]>]>,
    /// Its lifetime parameters', then its type parameters'.
    pub variances: Box<[Variance]>,
    /// What its parameters outlive, as its bounds say and its fields'
    /// types imply (rustc's inferred outlives requirements), in terms of
    /// its parameters: what a well-formed instance of it implies.
    pub outlives: Box<[Outlives<Region>]>,
}

/// The regions of a trait reference, beside its erased [`crate::TraitRef`].
/// Lifetime arguments are invariant and precede the type arguments in Rust;
/// they must not be confused with the region slots inside those types.
/// Preserving these tables does not establish that an obligation holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TraitRefRegions {
    /// Independently quantified non-callable lifetime slots (`Region::Late`).
    pub bound: u32,
    pub lifetimes: Box<[Region]>,
    pub args: Box<[Box<[Region]>]>,
}

/// A trait's lifetime parameters and the bounds its methods inherit.
/// One-owner required-method traits are admitted after their signature
/// metadata is checked; unsupported declarations retain the lifetime gate.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TraitRegions {
    /// Region rows aligned with TraitSig::supertraits.
    pub supertraits: Box<[TraitRefRegions]>,
    pub families: Box<[(Symbol, FamilyRegions)]>,
    pub names: Box<[LifetimeName]>,
    pub outlives: Box<[Outlives<Region>]>,
}

/// An associated family's contract, under its owner's lifetimes followed by
/// fresh family-local lifetime parameters. Type parameters follow the same
/// owner-then-family order in the type signature templates.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FamilyRegions {
    pub names: Box<[LifetimeName]>,
    pub owner: u32,
    /// The impl RHS's region row; empty for a trait declaration.
    pub value: Box<[Region]>,
    /// Where-clause prerequisites, not assumptions about an arbitrary use.
    pub outlives: Box<[Outlives<Region>]>,
    /// Explicit lifetime bounds on the declared output.
    pub output_outlives: Box<[Region]>,
    pub output_traits: Box<[(crate::TraitRef, TraitRefRegions)]>,
    pub output_projections: Box<[(crate::Projection, Box<[Region]>)]>,
    pub bound_selfs: Box<[Box<[Region]>]>,
    pub bound_traits: Box<[TraitRefRegions]>,
    pub projections: Box<[Box<[Region]>]>,
    pub selections: Box<[ProjectionSelection]>,
}

/// The lifetimes of an `impl` block's header.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImplRegions {
    pub families: Box<[(Symbol, FamilyRegions)]>,
    pub names: Box<[LifetimeName]>,
    pub self_ty: Box<[Region]>,
    /// Its trait's lifetime arguments, in terms of this impl's parameters.
    pub trait_lifetimes: Box<[Region]>,
    /// The slots of each of its trait's type arguments, after `Self`.
    pub trait_args: Box<[Box<[Region]>]>,
    /// Written projection inputs for Self, then each trait type argument.
    /// Each list preserves syntax occurrence order, including erased-equal
    /// projections with independently written lifetimes.
    pub header_selections: Box<[Box<[ProjectionSelection]>]>,
    /// Each associated type's.
    pub types: Box<[(Symbol, Box<[Region]>)]>,
    /// Projection inputs in associated values, before normalization can erase
    /// their selection obligations (including values with no region slots).
    pub type_selections: Box<[(Symbol, Box<[ProjectionSelection]>)]>,
    /// Its bounds, and what its header's types imply: what its items
    /// may assume.
    pub outlives: Box<[Outlives<Region>]>,
    /// Its bounds alone (`'b: 'a`, `T: 'a`): what a use of it must
    /// meet.
    pub bounds: Box<[Outlives<Region>]>,
    /// The slots of the types its bounds are on, as
    /// [`crate::Signatures::impl_predicates`] lists them.
    pub bound_selfs: Box<[Box<[Region]>]>,
    /// Each resolved predicate's trait arguments, in the same order.
    pub bound_traits: Box<[TraitRefRegions]>,
    /// The slots of what its bounds say associated types are, as
    /// [`crate::Signatures::impl_projections`] lists them.
    pub projections: Box<[Box<[Region]>]>,
}

/// A projection's selection inputs, independently of its normalized output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionSelection {
    pub alias: crate::AliasTy,
    /// This occurrence had an explicitly written qualified Self (`<T as Tr>`).
    /// Comparison-only consumers must not replay abstract/unqualified inputs.
    pub qualified: bool,
    /// Trait-owner arguments from this occurrence, before Self's region row.
    pub lifetimes: Box<[Region]>,
    /// Self followed by the trait's type arguments, beside `alias.args`.
    pub args: Box<[Box<[Region]>]>,
}

/// A type alias as a type and its slots, in terms of its parameters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AliasRegions {
    pub ty: Option<Ty>,
    pub regions: Box<[Region]>,
}

/// The lifetimes of every signature.
#[derive(Debug, Clone, Default)]
pub struct RegionSigs {
    pub traits: flux_arena::LayeredArenaMap<Trait, TraitRegions>,
    pub fns: flux_arena::LayeredArenaMap<Function, FnRegions>,
    pub structs: flux_arena::LayeredArenaMap<Struct, StructRegions>,
    pub impls: flux_arena::LayeredArenaMap<Impl, ImplRegions>,
    pub consts: flux_arena::LayeredArenaMap<Const, Box<[Region]>>,
}

impl RegionSigs {
    /// Struct `strukt`'s lifetimes, if known.
    pub fn strukt(&self, strukt: flux_hir::StructId) -> Option<&StructRegions> {
        self.structs.get(strukt)
    }
}

/// Only aliases with represented lifetime/family inputs have occurrence rows.
/// Legacy lifetimeless associated values keep their existing zero-slot model.
pub fn has_projection_inputs(alias: crate::AliasTy) -> bool {
    alias.owner_lifetimes != 0 || alias.family_lifetimes != 0 || !alias.family_args.is_empty()
}

/// Occurrence rows of a non-callable object's type and equality arguments.
/// Callable input regions remain bound at each call, not stored in the object.
pub fn object_region_types(items: &ItemTree, object: crate::DynTy) -> impl Iterator<Item = Ty> {
    let count = if crate::fn_trait_kind(items, object.trait_).is_some() {
        0
    } else {
        usize::MAX
    };
    object
        .args
        .iter()
        .chain(object.assoc_tys)
        .copied()
        .take(count)
}

/// The occurrence row of one declared object associated equality, after its
/// independent object bound and principal argument rows.
pub fn object_associated_range(
    items: &ItemTree,
    object: crate::DynTy,
    name: Symbol,
) -> Option<std::ops::Range<usize>> {
    if crate::fn_trait_kind(items, object.trait_).is_some() {
        return None;
    }
    let mut at = 1 + object
        .args
        .iter()
        .map(|&ty| slots(items, ty))
        .sum::<usize>();
    for (&associated, &ty) in object.assoc_names.iter().zip(object.assoc_tys) {
        let end = at + slots(items, ty);
        if associated == name {
            return Some(at..end);
        }
        at = end;
    }
    None
}

/// How many region slots `ty` has.
pub fn slots(items: &ItemTree, ty: Ty) -> usize {
    match ty.kind() {
        TyKind::Ref(_, t) => 1 + slots(items, t),
        TyKind::RawPtr(_, t) | TyKind::Array(t, _) | TyKind::Slice(t) => slots(items, t),
        TyKind::Tuple(list) => list.iter().map(|&t| slots(items, t)).sum(),
        TyKind::Adt(adt, args) => {
            items.structs[adt.strukt].generics.lifetimes.len()
                + args.iter().map(|&t| slots(items, t)).sum::<usize>()
        }
        TyKind::Alias(alias) if has_projection_inputs(alias) => {
            alias.owner_lifetimes as usize
                + alias.family_lifetimes as usize
                + alias
                    .args
                    .iter()
                    .chain(alias.family_args)
                    .map(|&ty| slots(items, ty))
                    .sum::<usize>()
        }
        TyKind::Closure(_, args) => slots(items, ClosureArgs::split(args).upvars),
        TyKind::Opaque(opaque) => {
            opaque.lifetimes as usize + opaque.args.iter().map(|&t| slots(items, t)).sum::<usize>()
        }
        TyKind::Dynamic(object) => {
            1 + object_region_types(items, object)
                .map(|ty| slots(items, ty))
                .sum::<usize>()
        }
        TyKind::FnPtr(sig) => sig.regions.map_or(0, |r| {
            r.free as usize + r.types.iter().map(|&ty| slots(items, ty)).sum::<usize>()
        }),
        _ => 0,
    }
}

/// The slots of an instance of `ty`, a type in some item's terms whose
/// own slots are `own`: `region` maps each of them, and type parameter
/// `j`'s slots are `args(j)`'s. Into `out`.
pub fn instantiate<R: Copy>(
    items: &ItemTree,
    ty: Ty,
    own: &[Region],
    region: &mut dyn FnMut(Region) -> R,
    args: &dyn Fn(u32) -> Vec<R>,
    out: &mut Vec<R>,
) {
    let mut next = 0;
    instantiate_at(items, ty, own, &mut next, region, args, out);
}

fn instantiate_at<R: Copy>(
    items: &ItemTree,
    ty: Ty,
    own: &[Region],
    next: &mut usize,
    region: &mut dyn FnMut(Region) -> R,
    args: &dyn Fn(u32) -> Vec<R>,
    out: &mut Vec<R>,
) {
    let mut own_slot = |next: &mut usize, out: &mut Vec<R>| {
        let r = own.get(*next).copied().unwrap_or(Region::Erased);
        *next += 1;
        out.push(region(r));
    };
    match ty.kind() {
        TyKind::Param(p) => out.extend(args(p.index)),
        TyKind::Ref(_, t) => {
            own_slot(next, out);
            instantiate_at(items, t, own, next, region, args, out);
        }
        TyKind::RawPtr(_, t) | TyKind::Array(t, _) | TyKind::Slice(t) => {
            instantiate_at(items, t, own, next, region, args, out)
        }
        TyKind::Tuple(list) => {
            for &t in list {
                instantiate_at(items, t, own, next, region, args, out);
            }
        }
        TyKind::Adt(adt, list) => {
            for _ in 0..items.structs[adt.strukt].generics.lifetimes.len() {
                own_slot(next, out);
            }
            for &t in list {
                instantiate_at(items, t, own, next, region, args, out);
            }
        }
        TyKind::Closure(_, list) => {
            let upvars = ClosureArgs::split(list).upvars;
            instantiate_at(items, upvars, own, next, region, args, out);
        }
        TyKind::Opaque(opaque) => {
            for _ in 0..opaque.lifetimes {
                own_slot(next, out);
            }
            for &t in opaque.args {
                instantiate_at(items, t, own, next, region, args, out);
            }
        }
        TyKind::Alias(alias) if has_projection_inputs(alias) => {
            for _ in 0..alias.owner_lifetimes {
                own_slot(next, out);
            }
            for &ty in alias.args {
                instantiate_at(items, ty, own, next, region, args, out);
            }
            for _ in 0..alias.family_lifetimes {
                let r = own.get(*next).copied().unwrap_or(Region::Erased);
                *next += 1;
                out.push(region(r));
            }
            for &ty in alias.family_args {
                instantiate_at(items, ty, own, next, region, args, out);
            }
        }
        TyKind::Dynamic(object) => {
            own_slot(next, out);
            for ty in object_region_types(items, object) {
                instantiate_at(items, ty, own, next, region, args, out);
            }
        }
        TyKind::FnPtr(sig) => {
            if let Some(regions) = sig.regions {
                for _ in 0..regions.free {
                    own_slot(next, out);
                }
                for &ty in regions.types {
                    instantiate_at(items, ty, own, next, region, args, out);
                }
            }
        }
        _ => {}
    }
}
