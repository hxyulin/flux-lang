//! `Box<T>`: a value on the heap, owned. It dereferences to the value,
//! and dropping it drops the value, then frees its memory.

use core::cmp::Ordering;
use core::fmt;
use core::ops::{Deref, DerefMut};
use core::ptr::{self, NonNull};

use crate::alloc::{Layout, alloc, dealloc, handle_alloc_error};

#[lang = "owned_box"]
pub struct Box<T> {
    ptr: NonNull<T>,
}

impl<T> Box<T> {
    /// `x`, moved to the heap; a value without size takes no memory.
    pub fn new(x: T) -> Box<T> {
        let layout = Layout::new::<T>();
        let ptr = if layout.size() == 0 {
            NonNull::dangling()
        } else {
            let raw = unsafe { alloc(layout) };
            if raw.is_null() {
                handle_alloc_error(layout);
            }
            unsafe { NonNull::new_unchecked(raw as *mut T) }
        };
        unsafe { ptr.write(x) };
        Box { ptr }
    }

    /// The box's pointer, which the caller now owns.
    pub fn into_raw(b: Box<T>) -> *mut T {
        let ptr = b.ptr.as_ptr();
        core::mem::forget(b);
        ptr
    }

    /// The box that owns `raw`, which `into_raw` gave.
    pub unsafe fn from_raw(raw: *mut T) -> Box<T> {
        Box {
            ptr: unsafe { NonNull::new_unchecked(raw) },
        }
    }

    /// The value, never freed.
    pub fn leak<'a>(b: Box<T>) -> &'a mut T {
        unsafe { &mut *Box::into_raw(b) }
    }
}

impl<T> Deref for Box<T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { self.ptr.as_ref() }
    }
}

impl<T> DerefMut for Box<T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { self.ptr.as_mut() }
    }
}

impl<T> Drop for Box<T> {
    fn drop(&mut self) {
        let layout = Layout::new::<T>();
        unsafe {
            ptr::drop_in_place(self.ptr.as_ptr());
            if layout.size() != 0 {
                dealloc(self.ptr.as_ptr() as *mut u8, layout);
            }
        }
    }
}

impl<T: Clone> Clone for Box<T> {
    fn clone(&self) -> Box<T> {
        Box::new((**self).clone())
    }
}

impl<T: Default> Default for Box<T> {
    fn default() -> Box<T> {
        Box::new(T::default())
    }
}

impl<T: PartialEq> PartialEq for Box<T> {
    fn eq(&self, other: &Box<T>) -> bool {
        **self == **other
    }
}

impl<T: Eq> Eq for Box<T> {}

impl<T: PartialOrd> PartialOrd for Box<T> {
    fn partial_cmp(&self, other: &Box<T>) -> Option<Ordering> {
        (**self).partial_cmp(&**other)
    }
}

impl<T: Ord> Ord for Box<T> {
    fn cmp(&self, other: &Box<T>) -> Ordering {
        (**self).cmp(&**other)
    }
}

impl<T: fmt::Debug> fmt::Debug for Box<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: fmt::Display> fmt::Display for Box<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T> From<T> for Box<T> {
    fn from(t: T) -> Box<T> {
        Box::new(t)
    }
}
