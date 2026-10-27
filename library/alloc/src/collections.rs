//! Ordered collections backed by the shared balanced tree.
pub mod binary_heap;
pub mod linked_list;
pub use binary_heap::BinaryHeap;
pub use linked_list::LinkedList;
pub mod btree_map;
pub mod btree_set;
pub use btree_map::BTreeMap;
pub use btree_set::BTreeSet;


/// An allocation failure from a collection's fallible reservation.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TryReserveError {
    kind: TryReserveErrorKind,
}

#[derive(Clone, PartialEq, Eq, Debug)]
enum TryReserveErrorKind {
    CapacityOverflow,
    AllocError { layout: crate::alloc::Layout, non_exhaustive: () },
}

impl TryReserveError {
    pub(crate) fn capacity_overflow() -> Self {
        Self { kind: TryReserveErrorKind::CapacityOverflow }
    }

    pub(crate) fn alloc_error(layout: crate::alloc::Layout) -> Self {
        Self { kind: TryReserveErrorKind::AllocError { layout, non_exhaustive: () } }
    }

    pub(crate) fn handle(self) -> ! {
        match self.kind {
            TryReserveErrorKind::CapacityOverflow => crate::raw_vec::capacity_overflow(),
            TryReserveErrorKind::AllocError { layout, .. } => crate::alloc::handle_alloc_error(layout),
        }
    }
}

impl core::fmt::Display for TryReserveError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("memory allocation failed")?;
        match self.kind {
            TryReserveErrorKind::CapacityOverflow =>
                f.write_str(" because the computed capacity exceeded the collection's maximum"),
            TryReserveErrorKind::AllocError { .. } =>
                f.write_str(" because the memory allocator returned an error"),
        }
    }
}

impl core::error::Error for TryReserveError {}
