//! Sized LP64 atomic storage, backed by native atomic instructions.
use crate::cell::UnsafeCell;
use crate::intrinsics;
use crate::marker::Sync;
use crate::panicking::panic;
use crate::result::Result::{self, Err, Ok};

/// Memory ordering for atomic operations and fences.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Ordering {
    /// No ordering of other memory accesses.
    Relaxed,
    /// Publishes prior accesses to an acquiring observer.
    Release,
    /// Observes accesses published by a releasing operation.
    Acquire,
    /// Both acquire and release ordering.
    AcqRel,
    /// Acquire/release ordering plus the sequentially consistent total order.
    SeqCst,
}
use self::Ordering::{AcqRel, Acquire, Relaxed, Release, SeqCst};

/// A naturally aligned, sized atomic machine word (64 bits on Flux targets).
/// Shared accesses use only native atomic operations; exclusive access may
/// use `get_mut` or `into_inner`. No wider or overlapping access is permitted.
#[repr(C, align(8))]
pub struct AtomicUsize {
    value: UnsafeCell<usize>,
}

// SAFETY: every shared access to the UnsafeCell is a native atomic operation.
// Send propagates structurally from UnsafeCell<usize>; no generic payload is hidden.
unsafe impl Sync for AtomicUsize {}

impl AtomicUsize {
    /// Initializes private storage; subsequent shared accesses are atomic.
    pub const fn new(value: usize) -> AtomicUsize {
        AtomicUsize {
            value: UnsafeCell::new(value),
        }
    }

    /// Exclusive access, with no concurrent atomic accesses possible.
    pub fn get_mut(&mut self) -> &mut usize {
        self.value.get_mut()
    }

    /// Consumes the atomic and returns its value.
    pub fn into_inner(self) -> usize {
        self.value.into_inner()
    }

    /// Loads with Relaxed, Acquire or SeqCst; other orders panic.
    #[track_caller]
    pub fn load(&self, order: Ordering) -> usize {
        // SAFETY: storage is initialized, aligned and accessed atomically.
        unsafe {
            match order {
                Relaxed => intrinsics::__atomic_usize_load_relaxed(self.value.get()),
                Acquire => intrinsics::__atomic_usize_load_acquire(self.value.get()),
                SeqCst => intrinsics::__atomic_usize_load_seqcst(self.value.get()),
                Release => panic("there is no such thing as a release load"),
                AcqRel => panic("there is no such thing as an acquire-release load"),
            }
        }
    }

    /// Stores with Relaxed, Release or SeqCst; other orders panic.
    #[track_caller]
    pub fn store(&self, value: usize, order: Ordering) {
        // SAFETY: storage is initialized, aligned and accessed atomically.
        unsafe {
            match order {
                Relaxed => intrinsics::__atomic_usize_store_relaxed(self.value.get(), value),
                Release => intrinsics::__atomic_usize_store_release(self.value.get(), value),
                SeqCst => intrinsics::__atomic_usize_store_seqcst(self.value.get(), value),
                Acquire => panic("there is no such thing as an acquire store"),
                AcqRel => panic("there is no such thing as an acquire-release store"),
            }
        }
    }

    /// Adds with wrapping arithmetic and returns the previous value.
    #[track_caller]
    pub fn fetch_add(&self, value: usize, order: Ordering) -> usize {
        // SAFETY: storage is initialized, aligned and accessed atomically.
        unsafe {
            match order {
                Relaxed => intrinsics::__atomic_usize_add_relaxed(self.value.get(), value),
                Acquire => intrinsics::__atomic_usize_add_acquire(self.value.get(), value),
                Release => intrinsics::__atomic_usize_add_release(self.value.get(), value),
                AcqRel => intrinsics::__atomic_usize_add_acqrel(self.value.get(), value),
                SeqCst => intrinsics::__atomic_usize_add_seqcst(self.value.get(), value),
            }
        }
    }

    /// Subtracts with wrapping arithmetic and returns the previous value.
    #[track_caller]
    pub fn fetch_sub(&self, value: usize, order: Ordering) -> usize {
        // SAFETY: storage is initialized, aligned and accessed atomically.
        unsafe {
            match order {
                Relaxed => intrinsics::__atomic_usize_sub_relaxed(self.value.get(), value),
                Acquire => intrinsics::__atomic_usize_sub_acquire(self.value.get(), value),
                Release => intrinsics::__atomic_usize_sub_release(self.value.get(), value),
                AcqRel => intrinsics::__atomic_usize_sub_acqrel(self.value.get(), value),
                SeqCst => intrinsics::__atomic_usize_sub_seqcst(self.value.get(), value),
            }
        }
    }

    /// Replaces `current`, returning the old value in Ok on success or Err
    /// on failure. Failure Release and AcqRel panic; all other pairs are valid.
    #[track_caller]
    pub fn compare_exchange(
        &self,
        current: usize,
        new: usize,
        success: Ordering,
        failure: Ordering,
    ) -> Result<usize, usize> {
        // Keel has one order: join both Rust orders without weakening either.
        // Validate failure before mapping; all fifteen remaining pairs are valid.
        let order = match failure {
            Release => panic("there is no such thing as a release failure ordering"),
            AcqRel => panic("there is no such thing as an acquire-release failure ordering"),
            SeqCst => SeqCst,
            Relaxed => success,
            Acquire => match success {
                Relaxed | Acquire => Acquire,
                Release | AcqRel => AcqRel,
                SeqCst => SeqCst,
            },
        };
        // SAFETY: aligned, live atomic storage. Pinned Keel CAS is strong and
        // returns the old scalar on BOTH paths, not a boolean or tuple.
        let old = unsafe {
            match order {
                Relaxed => intrinsics::__atomic_usize_cas_relaxed(self.value.get(), current, new),
                Acquire => intrinsics::__atomic_usize_cas_acquire(self.value.get(), current, new),
                Release => intrinsics::__atomic_usize_cas_release(self.value.get(), current, new),
                AcqRel => intrinsics::__atomic_usize_cas_acqrel(self.value.get(), current, new),
                SeqCst => intrinsics::__atomic_usize_cas_seqcst(self.value.get(), current, new),
            }
        };
        if old == current { Ok(old) } else { Err(old) }
    }

    /// This bounded implementation uses strong CAS, which is allowed for weak CAS.
    #[track_caller]
    pub fn compare_exchange_weak(
        &self,
        current: usize,
        new: usize,
        success: Ordering,
        failure: Ordering,
    ) -> Result<usize, usize> {
        self.compare_exchange(current, new, success, failure)
    }
}

/// An atomic fence. Relaxed is invalid and panics.
#[track_caller]
pub fn fence(order: Ordering) {
    // SAFETY: fences have no storage preconditions.
    unsafe {
        match order {
            Acquire => intrinsics::__atomic_usize_fence_acquire(),
            Release => intrinsics::__atomic_usize_fence_release(),
            AcqRel => intrinsics::__atomic_usize_fence_acqrel(),
            SeqCst => intrinsics::__atomic_usize_fence_seqcst(),
            Relaxed => panic("there is no such thing as a relaxed fence"),
        }
    }
}
