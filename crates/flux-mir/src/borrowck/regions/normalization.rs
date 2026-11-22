//! Normalize repeatedly under one borrow-checking environment.
use std::cell::RefCell;

use flux_hir::ItemTree;
use flux_ty::{Predicate, Projection, Signatures, Ty};
use rustc_hash::FxHashMap;

pub(super) struct Environment {
    projections: Vec<Projection>,
    memberships: Vec<Predicate>,
    values: RefCell<FxHashMap<Ty, Ty>>,
}

impl Environment {
    pub(super) fn new(projections: Vec<Projection>, memberships: Vec<Predicate>) -> Self {
        Self {
            projections,
            memberships,
            values: RefCell::default(),
        }
    }

    pub(super) fn projections(&self) -> &[Projection] {
        &self.projections
    }

    pub(super) fn projections_mut(&mut self) -> &mut Vec<Projection> {
        self.values.get_mut().clear();
        &mut self.projections
    }

    pub(super) fn memberships(&self) -> &[Predicate] {
        &self.memberships
    }

    pub(super) fn memberships_mut(&mut self) -> &mut Vec<Predicate> {
        self.values.get_mut().clear();
        &mut self.memberships
    }

    pub(super) fn normalize(&self, items: &ItemTree, sigs: &Signatures, ty: Ty) -> Ty {
        if !ty.has_aliases() && !ty.has_opaques() {
            return ty;
        }
        // The inputs are fixed for this Cx until one of the accessors above
        // changes the environment. Hits need only the interned type key.
        *self.values.borrow_mut().entry(ty).or_insert_with(|| {
            if ty.has_aliases() && !self.memberships.is_empty() {
                flux_solve::traits::normalize_with_memberships(
                    items,
                    sigs,
                    &self.projections,
                    &self.memberships,
                    ty,
                )
            } else {
                flux_solve::traits::normalize(items, sigs, &self.projections, ty)
            }
        })
    }
}
