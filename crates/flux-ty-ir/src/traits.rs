//! Predicates and the resolved headers of traits: what bounds say, in
//! terms of generic parameters. Resolving them from the item tree is
//! `flux-ty`'s; deciding whether they hold is `flux-solve`'s.

use std::fmt;

use flux_hir::TraitId;
use flux_intern::Symbol;

use crate::ty::{AliasTy, GenericArgs, Ty, TyKind};

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
            args: crate::ty::map_list(self.args, |t| t.subst(args)),
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

/// `<T as Tr>::Out == ty`: what an associated type is, from a bound's
/// `Tr<Out = ty>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Projection {
    pub alias: AliasTy,
    pub ty: Ty,
}

impl Projection {
    /// An exact written `A = A` adds no constraint. Test before normalization:
    /// following other equalities could make a real constraint certify itself.
    pub fn is_tautology(self) -> bool {
        self.ty == Ty::alias(self.alias)
    }

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

/// A family's own generics and declaration-local type contracts. Templates
/// index the owning trait/impl parameters before these own parameters.
#[derive(Debug, Clone, Default)]
pub struct AssociatedTypeSig {
    pub owner_params: u32,
    pub lifetimes: u32,
    pub params: Box<[Option<Ty>]>,
    pub predicates: Box<[Predicate]>,
    pub projections: Box<[Projection]>,
}

/// A trait's resolved header.
#[derive(Debug, Clone, Default)]
pub struct TraitSig {
    pub families: Box<[(Symbol, AssociatedTypeSig)]>,
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
    /// Header/where equalities, excluding associated-type declaration bounds.
    /// Kept separately because caller proof compatibility must not equate
    /// declaration assumptions with explicit where-clause assumptions.
    pub header_projections: Box<[Projection]>,
}

/// The default of each trait's parameters, for bounds that leave them out.
pub type TraitDefaults = flux_arena::LayeredArenaMap<flux_hir::Trait, Box<[Option<Ty>]>>;

/// Resolved bounds: what types implement, and what associated types are.
#[derive(Debug, Clone, Default)]
pub struct Bounds {
    pub predicates: Vec<Predicate>,
    pub projections: Vec<Projection>,
}
