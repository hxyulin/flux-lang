//! Atomic shared ownership of values and slices. The weak count includes one
//! implicit owner until the last strong owner has finished destroying T.
use core::marker::PhantomData;
use core::ops::Deref;
use core::ptr::{self, NonNull};
use core::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use core::sync::atomic::{AtomicUsize, fence};

use crate::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use crate::vec::Vec;

struct Header {
    strong: AtomicUsize,
    weak: AtomicUsize,
    layout: Layout,
}

/// Shared ownership of a value or slice. Cloning shares the allocation.
#[lang = "arc"]
pub struct Arc<T: ?Sized> {
    ptr: NonNull<Header>,
    data: NonNull<T>,
    owner: PhantomData<T>,
}

/// An allocation owner that does not keep its payload alive.
pub struct Weak<T: ?Sized> {
    ptr: Option<NonNull<Header>>,
    data: Option<NonNull<T>>,
    owner: PhantomData<T>,
}

// SAFETY: counters are atomic; accessing or transferring the shared payload
// requires both Send and Sync. Neither handle provides mutable payload access.
unsafe impl<T: ?Sized + Send + Sync> Send for Arc<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for Arc<T> {}
// SAFETY: upgrading shares T and the last strong owner may destroy T on any
// thread. The same Send + Sync requirements apply even to a weak handle.
unsafe impl<T: ?Sized + Send + Sync> Send for Weak<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for Weak<T> {}

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
        let (ptr, raw) = allocate(Layout::new::<T>());
        // SAFETY: aligned fresh payload storage, initialized before publication.
        unsafe {
            let data = raw as *mut T;
            ptr::write(data, value);
            Arc {
                ptr,
                data: NonNull::new_unchecked(data),
                owner: PhantomData,
            }
        }
    }
}

impl<T: ?Sized> Arc<T> {
    /// Creates a weak owner without extending the payload's lifetime.
    pub fn downgrade(this: &Arc<T>) -> Weak<T> {
        // SAFETY: a strong owner pins the header and its implicit weak count.
        increment(unsafe { &(*this.ptr.as_ptr()).weak });
        Weak {
            ptr: Some(this.ptr),
            data: Some(this.data),
            owner: PhantomData,
        }
    }

    /// Whether two handles share an allocation.
    pub fn ptr_eq(this: &Arc<T>, other: &Arc<T>) -> bool {
        this.ptr.as_ptr() == other.ptr.as_ptr()
    }
}

impl<T: ?Sized> Clone for Arc<T> {
    fn clone(&self) -> Arc<T> {
        // SAFETY: self pins the header and keeps strong above zero.
        increment(unsafe { &(*self.ptr.as_ptr()).strong });
        Arc {
            ptr: self.ptr,
            data: self.data,
            owner: PhantomData,
        }
    }
}

impl<T: ?Sized> Deref for Arc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: this strong owner keeps initialized T alive for the borrow.
        unsafe { self.data.as_ref() }
    }
}

impl<T: ?Sized> Drop for Arc<T> {
    fn drop(&mut self) {
        // SAFETY: self owns one strong count; only the last owner destroys T.
        unsafe {
            if (*self.ptr.as_ptr()).strong.fetch_sub(1, Release) != 1 {
                return;
            }
            fence(Acquire);
            // Retain the implicit weak through destruction, including reentrant
            // weak drops. A guard also releases it if unwinding is added later.
            let guard: Weak<T> = Weak {
                ptr: Some(self.ptr),
                data: Some(self.data),
                owner: PhantomData,
            };
            // Dynamic drop uses payload metadata, never a reference to the header.
            ptr::drop_in_place(self.data.as_ptr());
            drop(guard);
        }
    }
}

impl<T> Weak<T> {
    /// An empty weak handle: no allocation and no invalid payload value.
    pub const fn new() -> Weak<T> {
        Weak {
            ptr: None,
            data: None,
            owner: PhantomData,
        }
    }
}

impl<T: ?Sized> Weak<T> {
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
                        data: self.data.unwrap(),
                        owner: PhantomData,
                    });
                }
                Err(observed) => old = observed,
            }
        }
    }
}

impl<T: ?Sized> Clone for Weak<T> {
    fn clone(&self) -> Weak<T> {
        if let Some(ptr) = self.ptr {
            // SAFETY: self owns a weak count, pinning the header.
            increment(unsafe { &(*ptr.as_ptr()).weak });
        }
        Weak {
            ptr: self.ptr,
            data: self.data,
            owner: PhantomData,
        }
    }
}

impl<T: ?Sized> Drop for Weak<T> {
    fn drop(&mut self) {
        if let Some(ptr) = self.ptr {
            // SAFETY: only the last weak owner frees the header. It does not
            // access dead T or run drop glue for the payload a second time.
            unsafe {
                if (*ptr.as_ptr()).weak.fetch_sub(1, Release) != 1 {
                    return;
                }
                fence(Acquire);
                let layout = (*ptr.as_ptr()).layout;
                dealloc(ptr.as_ptr() as *mut u8, layout);
            }
        }
    }
}

// Header layout is erased and stable under future array-to-slice owner unsizing.
// Keep the original allocation layout alive independently of payload metadata.
fn allocate(payload: Layout) -> (NonNull<Header>, *mut u8) {
    let header = Layout::new::<Header>();
    let align = if header.align() > payload.align() {
        header.align()
    } else {
        payload.align()
    };
    let padded = match header.size().checked_add(payload.align() - 1) {
        Some(size) => size,
        None => crate::raw_vec::capacity_overflow(),
    };
    let offset = padded & !(payload.align() - 1);
    let size = match offset.checked_add(payload.size()) {
        Some(size) => size,
        None => crate::raw_vec::capacity_overflow(),
    };
    let layout = match Layout::from_size_align(size, align) {
        Ok(layout) => layout,
        Err(_) => crate::raw_vec::capacity_overflow(),
    };
    // SAFETY: the nonempty header guarantees a nonzero allocation size.
    unsafe {
        let base = alloc(layout);
        if base.is_null() {
            handle_alloc_error(layout);
        }
        let raw = base as *mut Header;
        ptr::write(
            raw,
            Header {
                strong: AtomicUsize::new(1),
                weak: AtomicUsize::new(1),
                layout,
            },
        );
        (NonNull::new_unchecked(raw), base.add(offset))
    }
}

impl<T> From<Vec<T>> for Arc<[T]> {
    fn from(mut values: Vec<T>) -> Arc<[T]> {
        let len = values.len();
        let layout = match Layout::array::<T>(len) {
            Ok(layout) => layout,
            Err(_) => crate::raw_vec::capacity_overflow(),
        };
        let (ptr, raw) = allocate(layout);
        // SAFETY: separate aligned storage for len elements; moving bytes then
        // clearing the source length transfers ownership without cloning. The
        // slice is formed only after all elements are initialized (also ZSTs).
        unsafe {
            let data = raw as *mut T;
            ptr::copy_nonoverlapping(values.as_ptr(), data, len);
            values.set_len(0);
            let slice = core::slice::from_raw_parts_mut(data, len);
            Arc {
                ptr,
                data: NonNull::new_unchecked(slice as *mut [T]),
                owner: PhantomData,
            }
        }
    }
}

impl<T> FromIterator<T> for Arc<[T]> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Arc<[T]> {
        let values: Vec<T> = iter.into_iter().collect();
        values.into()
    }
}
