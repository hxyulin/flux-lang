//! The memory of a `Vec`: room for `cap` values, grown as rustc's
//! `RawVec` grows it, so that `capacity()` agrees with rustc's: doubling,
//! to at least what is needed, and at least 8 elements of one byte, 4 of
//! up to 1024 bytes, or 1 otherwise. Zero-sized values need no memory,
//! and there is room for `usize::MAX` of them.

use core::ptr::NonNull;

use crate::alloc::{Layout, alloc, dealloc, handle_alloc_error, realloc};

pub(crate) struct RawVec<T> {
    ptr: NonNull<T>,
    cap: usize,
}

/// rustc's `RawVec::MIN_NON_ZERO_CAP`.
const fn min_non_zero_cap(size: usize) -> usize {
    if size == 1 {
        8
    } else if size <= 1024 {
        4
    } else {
        1
    }
}

/// More room than an `isize` can count, or a size that does not fit.
pub(crate) fn capacity_overflow() -> ! {
    panic!("capacity overflow")
}

impl<T> RawVec<T> {
    pub(crate) fn new() -> RawVec<T> {
        RawVec {
            ptr: NonNull::dangling(),
            cap: 0,
        }
    }

    /// Room for exactly `capacity` values.
    pub(crate) fn with_capacity(capacity: usize) -> RawVec<T> {
        if size_of::<T>() == 0 || capacity == 0 {
            return RawVec::new();
        }
        let layout = match Layout::array::<T>(capacity) {
            Ok(layout) => layout,
            Err(_) => capacity_overflow(),
        };
        let ptr = unsafe { alloc(layout) };
        if ptr.is_null() {
            handle_alloc_error(layout);
        }
        RawVec {
            ptr: unsafe { NonNull::new_unchecked(ptr as *mut T) },
            cap: capacity,
        }
    }

    /// Takes over `ptr`'s memory, room for `capacity` values.
    pub(crate) unsafe fn from_raw_parts(ptr: *mut T, capacity: usize) -> RawVec<T> {
        RawVec {
            ptr: unsafe { NonNull::new_unchecked(ptr) },
            cap: if size_of::<T>() == 0 { 0 } else { capacity },
        }
    }

    pub(crate) fn ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }

    pub(crate) fn capacity(&self) -> usize {
        if size_of::<T>() == 0 {
            usize::MAX
        } else {
            self.cap
        }
    }

    /// Room for `additional` more values after `len`, growing
    /// amortized.
    pub(crate) fn reserve(&mut self, len: usize, additional: usize) {
        if additional > self.capacity().wrapping_sub(len) {
            self.grow_amortized(len, additional);
        }
    }

    /// Room for `additional` more values after `len`, growing to exactly
    /// that.
    pub(crate) fn reserve_exact(&mut self, len: usize, additional: usize) {
        if additional > self.capacity().wrapping_sub(len) {
            if size_of::<T>() == 0 {
                capacity_overflow();
            }
            let cap = match len.checked_add(additional) {
                Some(cap) => cap,
                None => capacity_overflow(),
            };
            self.finish_grow(cap);
        }
    }

    /// Room for one more value, full as it is.
    pub(crate) fn grow_one(&mut self) {
        self.grow_amortized(self.cap, 1);
    }

    fn grow_amortized(&mut self, len: usize, additional: usize) {
        if size_of::<T>() == 0 {
            capacity_overflow();
        }
        let required = match len.checked_add(additional) {
            Some(required) => required,
            None => capacity_overflow(),
        };
        let doubled = self.cap * 2;
        let cap = if doubled > required { doubled } else { required };
        let min = min_non_zero_cap(size_of::<T>());
        let cap = if min > cap { min } else { cap };
        self.finish_grow(cap);
    }

    fn finish_grow(&mut self, cap: usize) {
        let new_layout = match Layout::array::<T>(cap) {
            Ok(layout) => layout,
            Err(_) => capacity_overflow(),
        };
        let ptr = if self.cap == 0 {
            unsafe { alloc(new_layout) }
        } else {
            let old_layout = unsafe {
                Layout::from_size_align_unchecked(self.cap * size_of::<T>(), align_of::<T>())
            };
            unsafe { realloc(self.ptr.as_ptr() as *mut u8, old_layout, new_layout.size()) }
        };
        if ptr.is_null() {
            handle_alloc_error(new_layout);
        }
        self.ptr = unsafe { NonNull::new_unchecked(ptr as *mut T) };
        self.cap = cap;
    }

    /// Room for exactly `cap` values, no more than there is.
    pub(crate) fn shrink_to_fit(&mut self, cap: usize) {
        if size_of::<T>() == 0 || cap >= self.cap {
            return;
        }
        let old_layout =
            unsafe { Layout::from_size_align_unchecked(self.cap * size_of::<T>(), align_of::<T>()) };
        if cap == 0 {
            unsafe { dealloc(self.ptr.as_ptr() as *mut u8, old_layout) };
            self.ptr = NonNull::dangling();
            self.cap = 0;
            return;
        }
        let new_size = cap * size_of::<T>();
        let ptr = unsafe { realloc(self.ptr.as_ptr() as *mut u8, old_layout, new_size) };
        if ptr.is_null() {
            handle_alloc_error(unsafe { Layout::from_size_align_unchecked(new_size, align_of::<T>()) });
        }
        self.ptr = unsafe { NonNull::new_unchecked(ptr as *mut T) };
        self.cap = cap;
    }
}

impl<T> Drop for RawVec<T> {
    fn drop(&mut self) {
        if self.cap != 0 && size_of::<T>() != 0 {
            unsafe {
                let layout =
                    Layout::from_size_align_unchecked(self.cap * size_of::<T>(), align_of::<T>());
                dealloc(self.ptr.as_ptr() as *mut u8, layout);
            }
        }
    }
}
