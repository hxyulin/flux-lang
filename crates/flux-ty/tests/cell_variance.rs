//! Check variance metadata directly, including cached core signatures.
use flux_ty::regions::Variance::{Covariant, Invariant};

#[test]
fn cell_seeds_propagate_without_changing_guards_or_owners() {
    let (_, _, hir) = flux_hir::lower_text(
        "cell_variance.rs",
        "use std::cell::{Cell, RefCell};
         use std::sync::{Arc, Weak};
         use std::marker::PhantomData;
         struct CellWrap<T>(Cell<T>);
         struct RefCellWrap<T>(RefCell<T>);
         struct ArcCell<T>(Arc<Cell<T>>);
         struct WeakRefCell<T>(Weak<RefCell<T>>);
         struct Marker<T>(PhantomData<Cell<T>>);
         struct Plain<T>(T);
         fn main() {}",
    );
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let (shared, errors) = flux_ty::signatures(&hir.items);
    assert!(errors.is_empty(), "{errors:?}");
    let (resolved, errors) = flux_ty::signatures_checked(&hir.items, true, None);
    assert!(errors.is_empty(), "{errors:?}");
    for (name, expected) in [
        ("UnsafeCell", vec![Invariant]),
        ("Cell", vec![Invariant]),
        ("RefCell", vec![Invariant]),
        ("Ref", vec![Covariant, Covariant]),
        ("RefMut", vec![Covariant, Invariant]),
        ("Arc", vec![Covariant]),
        ("Weak", vec![Covariant]),
        ("CellWrap", vec![Invariant]),
        ("RefCellWrap", vec![Invariant]),
        ("ArcCell", vec![Invariant]),
        ("WeakRefCell", vec![Invariant]),
        ("Marker", vec![Invariant]),
        ("Plain", vec![Covariant]),
    ] {
        let (id, _) = hir
            .items
            .structs
            .iter()
            .find(|(_, s)| s.name.to_string() == name)
            .unwrap();
        assert_eq!(&*shared.regions.structs[id].variances, expected, "{name}");
        assert_eq!(
            &*resolved.regions.structs[id].variances, expected,
            "fresh {name}"
        );
    }
}
