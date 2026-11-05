//! Memoized shape queries ([`super::is_memory`], [`super::leaves`],
//! [`super::leaf_count`]) for one compilation, by interned type.
//!
//! Each query recurses through the field types of a type and normalizes
//! them on every call. [`scope`] keeps their answers for one set of
//! signatures on this thread until the returned guard drops; the guard
//! borrows the signatures, so they cannot change meanwhile. Queries for
//! other signatures, or outside a scope, are computed directly.

use std::cell::RefCell;
use std::marker::PhantomData;

use flux_ty::{Signatures, Ty};
use rustc_hash::FxHashMap;

use super::TypeLeaves;

#[derive(Default)]
struct Memo {
    memory: FxHashMap<Ty, bool>,
    leaf_counts: FxHashMap<Ty, u32>,
    leaves: FxHashMap<Ty, Box<[Ty]>>,
}

struct Active {
    /// Identifies the signatures the answers are for; never dereferenced.
    sigs: *const Signatures,
    memo: Memo,
}

thread_local! {
    static ACTIVE: RefCell<Option<Active>> = const { RefCell::new(None) };
}

/// Keeps the answers for `sigs` until dropped, then restores the scope
/// that was active before.
pub(super) struct Scope<'a> {
    previous: Option<Active>,
    _sigs: PhantomData<&'a Signatures>,
}

pub(super) fn scope(sigs: &Signatures) -> Scope<'_> {
    let previous = ACTIVE.with(|active| {
        active.borrow_mut().replace(Active {
            sigs,
            memo: Memo::default(),
        })
    });
    Scope {
        previous,
        _sigs: PhantomData,
    }
}

impl Drop for Scope<'_> {
    fn drop(&mut self) {
        let previous = self.previous.take();
        ACTIVE.with(|active| *active.borrow_mut() = previous);
    }
}

/// Runs `f` on the memo of `sigs`, if a scope for them is active.
fn with_memo<R>(sigs: &Signatures, f: impl FnOnce(&mut Memo) -> R) -> Option<R> {
    ACTIVE.with(|active| match &mut *active.borrow_mut() {
        Some(active) if std::ptr::eq(active.sigs, sigs) => Some(f(&mut active.memo)),
        _ => None,
    })
}

pub(super) fn is_memory(sigs: &Signatures, ty: Ty, compute: fn(&Signatures, Ty) -> bool) -> bool {
    if let Some(Some(found)) = with_memo(sigs, |m| m.memory.get(&ty).copied()) {
        return found;
    }
    let found = compute(sigs, ty);
    with_memo(sigs, |m| m.memory.insert(ty, found));
    found
}

pub(super) fn leaf_count(
    sigs: &Signatures,
    ty: Ty,
    compute: fn(&Signatures, Ty) -> usize,
) -> usize {
    if let Some(Some(found)) = with_memo(sigs, |m| m.leaf_counts.get(&ty).copied()) {
        return found as usize;
    }
    let found = compute(sigs, ty);
    with_memo(sigs, |m| {
        m.leaf_counts
            .insert(ty, u32::try_from(found).expect("leaf counts fit 32 bits"))
    });
    found
}

pub(super) fn leaves(
    sigs: &Signatures,
    ty: Ty,
    out: &mut TypeLeaves,
    compute: fn(&Signatures, Ty, &mut TypeLeaves),
) {
    let found = with_memo(sigs, |m| {
        m.leaves
            .get(&ty)
            .map(|leaves| out.extend_from_slice(leaves))
    });
    match found {
        Some(Some(())) => {}
        // Outside a scope: nothing to keep.
        None => compute(sigs, ty, out),
        Some(None) => {
            let mut own = TypeLeaves::new();
            compute(sigs, ty, &mut own);
            out.extend_from_slice(&own);
            with_memo(sigs, |m| m.leaves.insert(ty, own.into_iter().collect()));
        }
    }
}
