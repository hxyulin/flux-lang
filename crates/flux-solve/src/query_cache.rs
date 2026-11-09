//! Completed root queries while their item tree and signatures are immutable.

use std::cell::RefCell;
use std::marker::PhantomData;

use flux_hir::ItemTree;
use flux_ty_ir::traits::{Predicate, Projection};
use rustc_hash::FxHashMap;

use crate::{Signatures, Ty, traits};

// Bound retained answers independently of the number of instantiated goals.
const MAX_QUERIES: usize = 65_536;

type Selected = Option<traits::SelectedImpl>;
type Normalized = FxHashMap<Box<[Projection]>, FxHashMap<Ty, Ty>>;

struct Cache {
    id: usize,
    items: *const ItemTree,
    sigs: *const Signatures,
    selected: FxHashMap<Predicate, Selected>,
    normalized: Normalized,
    entries: usize,
}

impl Cache {
    fn same(&self, items: &ItemTree, sigs: &Signatures) -> bool {
        std::ptr::eq(self.items, items) && std::ptr::eq(self.sigs, sigs)
    }

    fn room(&mut self) {
        if self.entries >= MAX_QUERIES {
            self.entries = 0;
            self.selected.clear();
            self.normalized.clear();
        }
        self.entries += 1;
    }
}

#[derive(Default)]
struct Scopes {
    next: usize,
    caches: Vec<Cache>,
}

thread_local! {
    static SCOPES: RefCell<Scopes> = RefCell::new(Scopes::default());
}

/// Borrows query inputs until dropped and discards the scope's cached answers.
/// Scopes are thread-local and can be dropped in any order.
pub struct SolverQueriesScope<'a> {
    id: usize,
    // A guard must be dropped on the thread that owns its cache.
    _inputs: PhantomData<(&'a ItemTree, &'a Signatures, *const ())>,
}

/// Reuses completed root selections and normalizations while both inputs are
/// borrowed. Each scope owns its cache; queries for other inputs bypass it.
/// Recursive selections keep their existing proof-local memoization.
pub fn remember_solver_queries<'a>(
    items: &'a ItemTree,
    sigs: &'a Signatures,
) -> SolverQueriesScope<'a> {
    let id = SCOPES.with_borrow_mut(|scopes| {
        scopes.next = scopes
            .next
            .checked_add(1)
            .expect("solver scope id overflow");
        let id = scopes.next;
        scopes.caches.push(Cache {
            id,
            items,
            sigs,
            selected: FxHashMap::default(),
            normalized: FxHashMap::default(),
            entries: 0,
        });
        id
    });
    SolverQueriesScope {
        id,
        _inputs: PhantomData,
    }
}

impl Drop for SolverQueriesScope<'_> {
    fn drop(&mut self) {
        SCOPES.with_borrow_mut(|scopes| {
            if let Some(index) = scopes.caches.iter().position(|cache| cache.id == self.id) {
                scopes.caches.remove(index);
            }
        });
    }
}

pub(crate) fn normalize(items: &ItemTree, sigs: &Signatures, env: &[Projection], ty: Ty) -> Ty {
    let cached = SCOPES.with_borrow(|scopes| {
        scopes
            .caches
            .last()
            .filter(|cache| cache.same(items, sigs))
            .map(|cache| {
                (
                    cache.id,
                    cache
                        .normalized
                        .get(env)
                        .and_then(|types| types.get(&ty))
                        .copied(),
                )
            })
    });
    if let Some((_, Some(known))) = cached {
        return known;
    }
    // No cache borrow crosses solver work, which can issue nested queries.
    let computed = traits::normalize_uncached(items, sigs, env, ty);
    if let Some((id, _)) = cached {
        SCOPES.with_borrow_mut(|scopes| {
            if let Some(cache) = scopes.caches.last_mut().filter(|cache| cache.id == id) {
                cache.room();
                cache
                    .normalized
                    .entry(env.into())
                    .or_default()
                    .insert(ty, computed);
            }
        });
    }
    computed
}

pub(crate) fn select(items: &ItemTree, sigs: &Signatures, goal: Predicate) -> Selected {
    let cached = SCOPES.with_borrow(|scopes| {
        scopes
            .caches
            .last()
            .filter(|cache| cache.same(items, sigs))
            .map(|cache| (cache.id, cache.selected.get(&goal).cloned()))
    });
    if let Some((_, Some(known))) = cached {
        return known;
    }
    let computed = traits::select_impl_uncached(items, sigs, goal);
    if let Some((id, _)) = cached {
        SCOPES.with_borrow_mut(|scopes| {
            if let Some(cache) = scopes.caches.last_mut().filter(|cache| cache.id == id) {
                cache.room();
                cache.selected.insert(goal, computed.clone());
            }
        });
    }
    computed
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_hir::SourceDb;
    use flux_intern::Symbol;
    use flux_ty_ir::traits::TraitRef;

    fn context() -> (ItemTree, Signatures, Predicate, Ty) {
        let mut db = SourceDb::new();
        let root = db.add(
            "queries.rs",
            "trait Value { type Out; } impl Value for bool { type Out = char; }",
        );
        let items = ItemTree::new(&mut db, root, Symbol::intern("queries"), false);
        let imp = items.impls.iter().next().unwrap().0;
        let trait_ = items.traits.iter().next().unwrap().0;
        let trait_ref = TraitRef {
            trait_,
            name: Symbol::intern("Value"),
            args: &[],
        };
        let mut sigs = Signatures::default();
        sigs.impls.insert(imp, Ty::BOOL);
        sigs.impl_traits.insert(imp, trait_ref);
        sigs.impl_types.insert(
            imp,
            vec![(Symbol::intern("Out"), Ty::CHAR)].into_boxed_slice(),
        );
        sigs.impl_index = crate::impl_index::ImplIndex::new(&items, &sigs);
        let goal = trait_ref.with_self(Ty::BOOL);
        let alias = Ty::alias(traits::alias_of(
            &items,
            trait_ref,
            Ty::BOOL,
            Symbol::intern("Out"),
        ));
        (items, sigs, goal, alias)
    }

    #[test]
    fn full_ordered_projection_environments_distinguish_answers() {
        let (items, sigs, _, alias) = context();
        let crate::TyKind::Alias(key) = alias.kind() else {
            panic!("expected alias");
        };
        let a = Projection {
            alias: key,
            ty: Ty::UNIT,
        };
        let b = Projection {
            alias: key,
            ty: Ty::BOOL,
        };
        let _scope = remember_solver_queries(&items, &sigs);
        for _ in 0..2 {
            assert_eq!(traits::normalize(&items, &sigs, &[], alias), Ty::CHAR);
            assert_eq!(traits::normalize(&items, &sigs, &[a], alias), Ty::UNIT);
            assert_eq!(traits::normalize(&items, &sigs, &[b], alias), Ty::BOOL);
            assert_eq!(traits::normalize(&items, &sigs, &[a, b], alias), Ty::UNIT);
            assert_eq!(traits::normalize(&items, &sigs, &[b, a], alias), Ty::BOOL);
            // Equal contents in another allocation still describe the same context.
            let owned = vec![a, b].into_boxed_slice();
            assert_eq!(traits::normalize(&items, &sigs, &owned, alias), Ty::UNIT);
        }
    }

    #[test]
    fn projection_cycles_do_not_publish_intermediate_answers() {
        let (items, sigs, goal, first) = context();
        let second = Ty::alias(traits::alias_of(
            &items,
            goal.trait_ref,
            Ty::CHAR,
            Symbol::intern("Out"),
        ));
        let (crate::TyKind::Alias(a), crate::TyKind::Alias(b)) = (first.kind(), second.kind())
        else {
            panic!("expected aliases");
        };
        let env = [
            Projection {
                alias: a,
                ty: second,
            },
            Projection {
                alias: b,
                ty: first,
            },
        ];
        let _scope = remember_solver_queries(&items, &sigs);
        for _ in 0..2 {
            assert_eq!(traits::normalize(&items, &sigs, &env, first), first);
            assert_eq!(traits::normalize(&items, &sigs, &env, second), second);
        }
    }

    #[test]
    fn negative_selections_and_normalizations_expire_before_signature_changes() {
        let (items, mut sigs, goal, alias) = context();
        let other = Predicate {
            self_ty: Ty::CHAR,
            ..goal
        };
        let imp = items.impls.iter().next().unwrap().0;
        {
            let _scope = remember_solver_queries(&items, &sigs);
            for _ in 0..2 {
                assert!(traits::select_impl(&items, &sigs, goal).is_some());
                assert!(traits::select_impl(&items, &sigs, other).is_none());
                assert_eq!(traits::normalize(&items, &sigs, &[], alias), Ty::CHAR);
            }
        }
        sigs.impls.insert(imp, Ty::CHAR);
        sigs.impl_types.insert(
            imp,
            vec![(Symbol::intern("Out"), Ty::UNIT)].into_boxed_slice(),
        );
        sigs.impl_index = crate::impl_index::ImplIndex::new(&items, &sigs);
        let _scope = remember_solver_queries(&items, &sigs);
        assert!(traits::select_impl(&items, &sigs, goal).is_none());
        assert!(traits::select_impl(&items, &sigs, other).is_some());
        assert_eq!(traits::normalize(&items, &sigs, &[], alias), alias);
    }

    #[test]
    fn nested_scopes_isolate_inputs_and_allow_non_lifo_drops() {
        let (items, mut first, goal, alias) = context();
        let imp = items.impls.iter().next().unwrap().0;
        let mut second = first.clone();
        second.impl_types.insert(
            imp,
            vec![(Symbol::intern("Out"), Ty::UNIT)].into_boxed_slice(),
        );
        let outer = remember_solver_queries(&items, &first);
        assert_eq!(traits::normalize(&items, &first, &[], alias), Ty::CHAR);
        {
            let _same = remember_solver_queries(&items, &first);
            assert!(traits::select_impl(&items, &first, goal).is_some());
        }
        let inner = remember_solver_queries(&items, &second);
        assert_eq!(traits::normalize(&items, &second, &[], alias), Ty::UNIT);
        assert_eq!(traits::normalize(&items, &first, &[], alias), Ty::CHAR);
        drop(outer);
        first.impl_types.insert(
            imp,
            vec![(Symbol::intern("Out"), Ty::BOOL)].into_boxed_slice(),
        );
        assert_eq!(traits::normalize(&items, &first, &[], alias), Ty::BOOL);
        drop(inner);
        // Dropping the inner guard must not restore the expired first cache.
        assert_eq!(traits::normalize(&items, &first, &[], alias), Ty::BOOL);
        let _scope = remember_solver_queries(&items, &first);
        assert_eq!(traits::normalize(&items, &first, &[], alias), Ty::BOOL);
    }

    #[test]
    fn query_capacity_eviction_preserves_answers() {
        let (items, sigs, goal, alias) = context();
        let _scope = remember_solver_queries(&items, &sigs);
        let expected = traits::select_impl(&items, &sigs, goal);
        for index in 0..MAX_QUERIES as u32 {
            let other = Predicate {
                self_ty: Ty::param(index, Symbol::intern("T")),
                ..goal
            };
            assert!(traits::select_impl(&items, &sigs, other).is_none());
        }
        assert_eq!(traits::select_impl(&items, &sigs, goal), expected);
        assert_eq!(traits::normalize(&items, &sigs, &[], alias), Ty::CHAR);
    }
}
