//! Structural impl-header matches during phases with fixed signatures.

use std::cell::RefCell;
use std::marker::PhantomData;

use flux_hir::{ImplId, ItemTree};
use rustc_hash::FxHashMap;

use crate::{Predicate, Signatures, Ty, traits};

// Bound memory independently of the number of monomorphized goals. Clearing
// loses only reuse; it cannot change a match's answer.
const MAX_MATCHES: usize = 65_536;

type HeaderMatches = FxHashMap<(ImplId, Predicate, bool), Option<Box<[Ty]>>>;

struct Remembered {
    items: *const ItemTree,
    sigs: *const Signatures,
    matches: HeaderMatches,
}

thread_local! {
    static REMEMBERED: RefCell<Option<Remembered>> = const { RefCell::new(None) };
}

/// Keeps structural header matches on this thread while both inputs are
/// borrowed. Dropping the scope restores the preceding cache, if any.
pub struct ImplHeadersScope<'a> {
    previous: Option<Option<Remembered>>,
    _inputs: PhantomData<(&'a ItemTree, &'a Signatures)>,
}

/// Reuses structural matching until the returned scope drops. Projection
/// normalization and recursive trait selection remain uncached.
pub fn remember_impl_headers<'a>(
    items: &'a ItemTree,
    sigs: &'a Signatures,
) -> ImplHeadersScope<'a> {
    let previous = REMEMBERED.with_borrow_mut(|remembered| {
        if remembered
            .as_ref()
            .is_some_and(|r| std::ptr::eq(r.items, items) && std::ptr::eq(r.sigs, sigs))
        {
            return None;
        }
        Some(remembered.replace(Remembered {
            items,
            sigs,
            matches: FxHashMap::default(),
        }))
    });
    ImplHeadersScope {
        previous,
        _inputs: PhantomData,
    }
}

impl Drop for ImplHeadersScope<'_> {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            REMEMBERED.with_borrow_mut(|remembered| *remembered = previous);
        }
    }
}

/// Headers with projection arguments or projection-inferred bindings retain
/// the original path: their answer can depend on recursive selection state.
pub(crate) fn match_impl(
    items: &ItemTree,
    sigs: &Signatures,
    imp: ImplId,
    goal: Predicate,
    sized: bool,
) -> Option<Vec<Ty>> {
    let key = (imp, goal, sized);
    let known = REMEMBERED.with_borrow(|remembered| {
        remembered
            .as_ref()
            .filter(|r| std::ptr::eq(r.items, items) && std::ptr::eq(r.sigs, sigs))
            .map(|r| r.matches.get(&key).cloned())
    });
    if let Some(Some(known)) = known {
        return known.map(|args| args.into_vec());
    }
    let computed = traits::match_impl_sized_uncached(items, sigs, imp, goal, sized);
    if known.is_some()
        && sigs.impls.get(imp).is_none_or(|ty| !ty.has_aliases())
        && sigs
            .impl_traits
            .get(imp)
            .is_some_and(|t| !t.args.iter().any(|ty| ty.has_aliases()))
        && sigs.impl_projections.get(imp).is_none_or(|p| p.is_empty())
    {
        REMEMBERED.with_borrow_mut(|remembered| {
            let Some(cache) = remembered.as_mut() else {
                return;
            };
            if cache.matches.len() >= MAX_MATCHES {
                cache.matches.clear();
            }
            cache
                .matches
                .insert(key, computed.as_deref().map(Box::from));
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

    fn context() -> (ItemTree, Signatures, ImplId) {
        let mut db = SourceDb::new();
        let root = db.add(
            "headers.rs",
            "trait Header<A> {} impl<T> Header<T> for (T, T) {}",
        );
        let items = ItemTree::new(&mut db, root, Symbol::intern("headers"), false);
        let imp = items.impls.iter().next().unwrap().0;
        let trait_ = items.traits.iter().next().unwrap().0;
        let param = Ty::param(0, Symbol::intern("T"));
        let mut sigs = Signatures::default();
        sigs.impls.insert(imp, Ty::tuple(&[param, param]));
        sigs.impl_traits.insert(
            imp,
            TraitRef {
                trait_,
                name: Symbol::intern("Header"),
                args: Ty::list(&[param]),
            },
        );
        (items, sigs, imp)
    }

    fn goal(sigs: &Signatures, imp: ImplId, left: Ty, right: Ty, arg: Ty) -> Predicate {
        Predicate {
            self_ty: Ty::tuple(&[left, right]),
            trait_ref: flux_ty_ir::traits::TraitRef {
                args: Ty::list(&[arg]),
                ..sigs.impl_traits[imp]
            },
        }
    }

    #[test]
    fn repeated_parameters_and_arguments_must_agree() {
        let (items, sigs, imp) = context();
        let _scope = remember_impl_headers(&items, &sigs);
        for _ in 0..2 {
            let yes = goal(&sigs, imp, Ty::BOOL, Ty::BOOL, Ty::BOOL);
            assert_eq!(
                match_impl(&items, &sigs, imp, yes, true),
                Some(vec![Ty::BOOL])
            );
            let no = goal(&sigs, imp, Ty::BOOL, Ty::CHAR, Ty::BOOL);
            assert_eq!(match_impl(&items, &sigs, imp, no, true), None);
            let no = goal(&sigs, imp, Ty::BOOL, Ty::BOOL, Ty::CHAR);
            assert_eq!(match_impl(&items, &sigs, imp, no, true), None);
        }
    }

    #[test]
    fn sized_mode_is_part_of_the_answer() {
        let (items, sigs, imp) = context();
        let _scope = remember_impl_headers(&items, &sigs);
        let query = goal(&sigs, imp, Ty::STR, Ty::STR, Ty::STR);
        for _ in 0..2 {
            assert_eq!(
                match_impl(&items, &sigs, imp, query, false),
                Some(vec![Ty::STR])
            );
            assert_eq!(match_impl(&items, &sigs, imp, query, true), None);
        }
    }

    #[test]
    fn projection_arguments_are_not_remembered() {
        let (items, mut sigs, imp) = context();
        let alias = traits::alias_of(
            &items,
            sigs.impl_traits[imp],
            Ty::param(0, Symbol::intern("T")),
            Symbol::intern("Value"),
        );
        sigs.impl_traits.get_mut(imp).unwrap().args = Ty::list(&[Ty::alias(alias)]);
        let query = goal(&sigs, imp, Ty::BOOL, Ty::BOOL, Ty::BOOL);
        let _scope = remember_impl_headers(&items, &sigs);
        for _ in 0..2 {
            assert_eq!(
                match_impl(&items, &sigs, imp, query, true),
                traits::match_impl_sized_uncached(&items, &sigs, imp, query, true)
            );
        }
        REMEMBERED.with_borrow(|remembered| {
            assert!(remembered.as_ref().unwrap().matches.is_empty());
        });
    }

    #[test]
    fn nested_scopes_and_changed_signatures_do_not_share_answers() {
        let (items, mut first, imp) = context();
        let mut second = first.clone();
        second.impls.insert(imp, Ty::CHAR);
        let query = goal(&first, imp, Ty::BOOL, Ty::BOOL, Ty::BOOL);
        {
            let _outer = remember_impl_headers(&items, &first);
            assert!(match_impl(&items, &first, imp, query, true).is_some());
            {
                let _inner = remember_impl_headers(&items, &second);
                assert!(match_impl(&items, &second, imp, query, true).is_none());
                assert!(match_impl(&items, &first, imp, query, true).is_some());
            }
            let _same = remember_impl_headers(&items, &first);
            assert!(match_impl(&items, &first, imp, query, true).is_some());
        }
        first.impls.insert(imp, Ty::CHAR);
        let _scope = remember_impl_headers(&items, &first);
        assert!(match_impl(&items, &first, imp, query, true).is_none());
    }
}
