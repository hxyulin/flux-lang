//! The `impl` blocks of each trait by the outermost constructor of their
//! type, as rustc's `SimplifiedType` has them: a goal, or a block checked
//! for overlap, need only look at the blocks for types with its
//! constructor, and at those that may be for any type.

use flux_hir::{FunctionId, ImplId, ItemTree, StructId, TraitId};
use rustc_hash::FxHashMap;

use crate::{IntTy, Mutability, Signatures, Ty, TyKind};

/// The outermost constructor of a type, when it has one that no other
/// type it does not unify with shares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Head {
    Int(IntTy),
    Bool,
    Tuple(u32),
    Adt(StructId),
    Ref(Mutability),
    RawPtr(Mutability),
    Array(u64),
    Slice,
    FnDef(FunctionId),
    FnPtr,
    Never,
    CStr,
}

/// The constructor of `ty`, or none if it may unify with types of any:
/// a parameter, an associated type, a variable or an error.
pub(crate) fn head(ty: Ty) -> Option<Head> {
    Some(match ty.kind() {
        TyKind::Int(int) => Head::Int(int),
        TyKind::Bool => Head::Bool,
        TyKind::Tuple(list) => Head::Tuple(list.len() as u32),
        TyKind::Adt(adt, _) => Head::Adt(adt.strukt),
        TyKind::Ref(m, _) => Head::Ref(m),
        TyKind::RawPtr(m, _) => Head::RawPtr(m),
        TyKind::Array(_, len) => Head::Array(len),
        TyKind::Slice(_) => Head::Slice,
        TyKind::FnDef(f) => Head::FnDef(f.function),
        TyKind::FnPtr(_) => Head::FnPtr,
        TyKind::Never => Head::Never,
        TyKind::CStr => Head::CStr,
        // Any other kind may stand for, or unify with, any type.
        _ => return None,
    })
}

/// The `impl` blocks of each trait, as positions in its
/// [`ItemTree::trait_impls`] entry, by the constructor of their type.
#[derive(Debug, Clone, Default)]
pub struct ImplIndex {
    /// The blocks for types with each constructor, in source order.
    by_head: FxHashMap<(TraitId, Head), Vec<u32>>,
    /// The blocks that may be for any type (`impl<T> Tr for T`), or whose
    /// type did not resolve, in source order.
    blanket: FxHashMap<TraitId, Vec<u32>>,
}

impl ImplIndex {
    /// Indexes the `impl` blocks of every trait in `items` by the types
    /// `sigs` has for them.
    pub fn new(items: &ItemTree, sigs: &Signatures) -> ImplIndex {
        let mut index = ImplIndex::default();
        for (&t, impls) in &items.trait_impls {
            for (position, &imp) in impls.iter().enumerate() {
                let list = match sigs.impls.get(imp).copied().and_then(head) {
                    Some(head) => index.by_head.entry((t, head)).or_default(),
                    None => index.blanket.entry(t).or_default(),
                };
                list.push(position as u32);
            }
        }
        index
    }

    /// The `impl` blocks of `t` that may be for `ty`, as far as their
    /// outermost constructors tell, in source order.
    pub fn impls_for<'s>(&'s self, items: &'s ItemTree, t: TraitId, ty: Ty) -> ImplsFor<'s> {
        self.impls_with_head(items, t, head(ty))
    }

    /// The `impl` blocks of `t` that may be for a type with constructor
    /// `head`, or for any type without one, in source order.
    pub(crate) fn impls_with_head<'s>(
        &'s self,
        items: &'s ItemTree,
        t: TraitId,
        head: Option<Head>,
    ) -> ImplsFor<'s> {
        let impls = items.trait_impls.get(&t).map_or(&[][..], |v| v);
        match head {
            Some(head) => self.positions(impls, t, head, usize::MAX),
            None => ImplsFor {
                impls,
                some: None,
                blanket: &[],
                next: 0,
            },
        }
    }

    /// The `impl` blocks of `t` before the one at `position` in its
    /// [`ItemTree::trait_impls`] entry that may be for the same types as
    /// that one, as far as their outermost constructors tell, in source
    /// order.
    pub(crate) fn earlier<'s>(
        &'s self,
        items: &'s ItemTree,
        sigs: &Signatures,
        t: TraitId,
        position: usize,
    ) -> ImplsFor<'s> {
        let impls = items.trait_impls.get(&t).map_or(&[][..], |v| v);
        match sigs.impls.get(impls[position]).copied().and_then(head) {
            Some(head) => self.positions(impls, t, head, position),
            None => ImplsFor {
                impls: &impls[..position],
                some: None,
                blanket: &[],
                next: 0,
            },
        }
    }

    /// The blocks of `t` among `impls` for types with constructor `head`,
    /// or for any type, before `end`.
    fn positions<'s>(
        &'s self,
        impls: &'s [ImplId],
        t: TraitId,
        head: Head,
        end: usize,
    ) -> ImplsFor<'s> {
        let before = |list: &'s Vec<u32>| {
            let count = list.partition_point(|&p| (p as usize) < end);
            &list[..count]
        };
        ImplsFor {
            impls,
            some: Some(self.by_head.get(&(t, head)).map_or(&[][..], before)),
            blanket: self.blanket.get(&t).map_or(&[][..], before),
            next: 0,
        }
    }
}

/// The `impl` blocks [`ImplIndex::impls_for`] gives: all of `impls`
/// from `next` on, without `some`, or else those at the positions in
/// `some` and `blanket`, merged in source order.
#[derive(Clone)]
pub struct ImplsFor<'s> {
    impls: &'s [ImplId],
    some: Option<&'s [u32]>,
    blanket: &'s [u32],
    next: usize,
}

impl Iterator for ImplsFor<'_> {
    type Item = ImplId;

    fn next(&mut self) -> Option<ImplId> {
        let position = match &mut self.some {
            None => {
                let position = self.next;
                self.next += 1;
                position
            }
            Some(some) => match (some.first(), self.blanket.first()) {
                (Some(&a), Some(&b)) if b < a => {
                    self.blanket = &self.blanket[1..];
                    b as usize
                }
                (Some(&a), _) => {
                    *some = &some[1..];
                    a as usize
                }
                (None, Some(&b)) => {
                    self.blanket = &self.blanket[1..];
                    b as usize
                }
                (None, None) => return None,
            },
        };
        self.impls.get(position).copied()
    }
}
