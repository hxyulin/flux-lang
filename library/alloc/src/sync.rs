//! Atomic shared ownership of sized values. The weak count includes one
//! implicit owner until the last strong owner has finished destroying T.
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ops::Deref;
use core::ptr::{self, NonNull};
use core::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use core::sync::atomic::{AtomicUsize, fence};

use crate::alloc::{Layout, alloc, dealloc, handle_alloc_error};

struct Inner<T> {
    strong: AtomicUsize,
    weak: AtomicUsize,
    data: MaybeUninit<T>,
}

/// Shared ownership of a sized value. Cloning shares the allocation.
pub struct Arc<T> {
    ptr: NonNull<Inner<T>>,
    owner: PhantomData<T>,
}

/// An allocation owner that does not keep its payload alive.
pub struct Weak<T> {
    ptr: Option<NonNull<Inner<T>>>,
    owner: PhantomData<T>,
}

// SAFETY: counters are atomic; accessing or transferring the shared payload
// requires both Send and Sync. Neither handle provides mutable payload access.
unsafe impl<T: Send + Sync> Send for Arc<T> {}
unsafe impl<T: Send + Sync> Sync for Arc<T> {}
// SAFETY: upgrading shares T and the last strong owner may destroy T on any
// thread. The same Send + Sync requirements apply even to a weak handle.
unsafe impl<T: Send + Sync> Send for Weak<T> {}
unsafe impl<T: Send + Sync> Sync for Weak<T> {}

// Check before CAS: even leaked or concurrently cloned handles cannot wrap.
// No uniqueness APIs use the weak counter, so it needs no lock sentinel.
fn increment(counter: &AtomicUsize) {
    let mut old = counter.load(Relaxed);
    loop {
        if old >= isize::MAX as usize {
            core::panicking::abort_now();
        }
        match counter.compare_exchange_weak(old, old + 1, Relaxed, Relaxed) {
            Ok(_) => return,
            Err(observed) => old = observed,
        }
    }
}

impl<T> Arc<T> {
    /// Moves a value into a new allocation, including for zero-sized T.
    pub fn new(value: T) -> Arc<T> {
        let layout = Layout::new::<Inner<T>>();
        // SAFETY: layout is nonzero because the header contains two counters.
        let raw = unsafe { alloc(layout) } as *mut Inner<T>;
        if raw.is_null() {
            handle_alloc_error(layout);
        }
        // SAFETY: fresh, correctly aligned storage; initialize before publishing.
        unsafe {
            ptr::write(
                raw,
                Inner {
                    strong: AtomicUsize::new(1),
                    weak: AtomicUsize::new(1),
                    data: MaybeUninit::new(value),
                },
            );
            Arc {
                ptr: NonNull::new_unchecked(raw),
                owner: PhantomData,
            }
        }
    }

    /// Creates a weak owner without extending the payload's lifetime.
    pub fn downgrade(this: &Arc<T>) -> Weak<T> {
        // SAFETY: a strong owner pins the header and its implicit weak count.
        increment(unsafe { &(*this.ptr.as_ptr()).weak });
        Weak {
            ptr: Some(this.ptr),
            owner: PhantomData,
        }
    }

    /// Whether two handles share an allocation.
    pub fn ptr_eq(this: &Arc<T>, other: &Arc<T>) -> bool {
        this.ptr.as_ptr() == other.ptr.as_ptr()
    }
}

impl<T> Clone for Arc<T> {
    fn clone(&self) -> Arc<T> {
        // SAFETY: self pins the header and keeps strong above zero.
        increment(unsafe { &(*self.ptr.as_ptr()).strong });
        Arc {
            ptr: self.ptr,
            owner: PhantomData,
        }
    }
}

impl<T> Deref for Arc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: this strong owner keeps initialized T alive for the borrow.
        unsafe { (*self.ptr.as_ptr()).data.assume_init_ref() }
    }
}

impl<T> Drop for Arc<T> {
    fn drop(&mut self) {
        // SAFETY: self owns one strong count; only the last owner destroys T.
        unsafe {
            if (*self.ptr.as_ptr()).strong.fetch_sub(1, Release) != 1 {
                return;
            }
            fence(Acquire);
            // Retain the implicit weak through destruction, including reentrant
            // weak drops. A guard also releases it if unwinding is added later.
            let guard = Weak {
                ptr: Some(self.ptr),
                owner: PhantomData,
            };
            // Use a raw field pointer, without an exclusive header reference
            // that would conflict with concurrently accessed atomic counters.
            ptr::drop_in_place(ptr::addr_of_mut!((*self.ptr.as_ptr()).data).cast::<T>());
            drop(guard);
        }
    }
}

impl<T> Weak<T> {
    /// An empty weak handle: no allocation and no invalid payload value.
    pub const fn new() -> Weak<T> {
        Weak {
            ptr: None,
            owner: PhantomData,
        }
    }

    /// Acquires a strong owner only while the payload is still alive.
    pub fn upgrade(&self) -> Option<Arc<T>> {
        let ptr = self.ptr?;
        // SAFETY: this weak owner pins the initialized header, even after T dies.
        let strong = unsafe { &(*ptr.as_ptr()).strong };
        let mut old = strong.load(Relaxed);
        loop {
            if old == 0 {
                return None;
            }
            if old >= isize::MAX as usize {
                core::panicking::abort_now();
            }
            match strong.compare_exchange_weak(old, old + 1, Acquire, Relaxed) {
                Ok(_) => {
                    return Some(Arc {
                        ptr,
                        owner: PhantomData,
                    });
                }
                Err(observed) => old = observed,
            }
        }
    }
}

impl<T> Clone for Weak<T> {
    fn clone(&self) -> Weak<T> {
        if let Some(ptr) = self.ptr {
            // SAFETY: self owns a weak count, pinning the header.
            increment(unsafe { &(*ptr.as_ptr()).weak });
        }
        Weak {
            ptr: self.ptr,
            owner: PhantomData,
        }
    }
}

impl<T> Drop for Weak<T> {
    fn drop(&mut self) {
        if let Some(ptr) = self.ptr {
            // SAFETY: only the last weak owner frees the header. It does not
            // access dead T or run drop glue for Inner<T> a second time.
            unsafe {
                if (*ptr.as_ptr()).weak.fetch_sub(1, Release) != 1 {
                    return;
                }
                fence(Acquire);
                dealloc(ptr.as_ptr() as *mut u8, Layout::new::<Inner<T>>());
            }
        }
    }
}
