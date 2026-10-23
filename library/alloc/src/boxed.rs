//! `Box<T>`: a value on the heap, owned. It dereferences to the value
//! as a built-in pointer does (`#[lang = "owned_box"]`): `*b` may be
//! moved out of, and is mutable where `b` is. Dropping it drops the
//! value (its `Drop`), then frees its memory (its field's); a box whose
//! value was moved out of only frees it.

use core::cmp::Ordering;
use core::fmt;
use core::ops::{Deref, DerefMut};
use core::ptr::{self, NonNull};

use crate::alloc::{Layout, alloc, dealloc, handle_alloc_error};

#[lang = "owned_box"]
pub struct Box<T: ?Sized> {
    ptr: Unique<T>,
}

/// A box's memory, which dropping frees.
struct Unique<T: ?Sized> {
    ptr: NonNull<T>,
}

impl<T: ?Sized> Drop for Unique<T> {
    /// Its size, of a slice or trait object too, read from where it
    /// points after its value was dropped or moved out.
    fn drop(&mut self) {
        let layout = unsafe { Layout::for_value(self.ptr.as_ref()) };
        if layout.size() != 0 {
            unsafe { dealloc(self.ptr.as_ptr() as *mut u8, layout) };
        }
    }
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
        Box {
            ptr: Unique { ptr },
        }
    }
}

impl<T: ?Sized> Box<T> {
    /// The box's pointer, which the caller now owns.
    pub fn into_raw(b: Box<T>) -> *mut T {
        let ptr = b.ptr.ptr.as_ptr();
        core::mem::forget(b);
        ptr
    }

    /// The box that owns `raw`, which `into_raw` gave.
    pub unsafe fn from_raw(raw: *mut T) -> Box<T> {
        Box {
            ptr: Unique {
                ptr: unsafe { NonNull::new_unchecked(raw) },
            },
        }
    }

    /// The value, never freed.
    pub fn leak<'a>(b: Box<T>) -> &'a mut T {
        unsafe { &mut *Box::into_raw(b) }
    }
}

impl<T: ?Sized> Deref for Box<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &**self
    }
}

impl<T: ?Sized> DerefMut for Box<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut **self
    }
}

/// The value; the field frees the memory after.
impl<T: ?Sized> Drop for Box<T> {
    fn drop(&mut self) {
        unsafe { ptr::drop_in_place(self.ptr.ptr.as_ptr()) };
    }
}

impl<T: Clone> Clone for Box<T> {
    fn clone(&self) -> Box<T> {
        Box::new((**self).clone())
    }
}

impl<T: Clone> Clone for Box<[T]> {
    fn clone(&self) -> Box<[T]> {
        self.to_vec().into_boxed_slice()
    }
}

impl Clone for Box<str> {
    fn clone(&self) -> Box<str> {
        Box::from(&**self)
    }
}

impl From<&str> for Box<str> {
    fn from(s: &str) -> Box<str> {
        let bytes = s.as_bytes().to_vec().into_boxed_slice();
        // SAFETY: the bytes came from a string, so remain valid UTF-8. A boxed
        // byte slice and a boxed string have the same allocation layout.
        unsafe { Box::from_raw(Box::into_raw(bytes) as *mut str) }
    }
}

impl<T> FromIterator<T> for Box<[T]> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Box<[T]> {
        crate::vec::Vec::from_iter(iter).into_boxed_slice()
    }
}

impl<T: Default> Default for Box<T> {
    fn default() -> Box<T> {
        Box::new(T::default())
    }
}

impl<T: ?Sized + PartialEq> PartialEq for Box<T> {
    fn eq(&self, other: &Box<T>) -> bool {
        **self == **other
    }
}

impl<T: ?Sized + Eq> Eq for Box<T> {}

impl<T: ?Sized + PartialOrd> PartialOrd for Box<T> {
    fn partial_cmp(&self, other: &Box<T>) -> Option<Ordering> {
        (**self).partial_cmp(&**other)
    }
}

impl<T: ?Sized + Ord> Ord for Box<T> {
    fn cmp(&self, other: &Box<T>) -> Ordering {
        (**self).cmp(&**other)
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for Box<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for Box<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T> Box<[T]> {
    /// Room for `len` elements, without initializing or dropping any `T`.
    /// Zero-sized elements require no allocation, but retain their length.
    pub fn new_uninit_slice(len: usize) -> Box<[core::mem::MaybeUninit<T>]> {
        let mut values = crate::vec::Vec::<core::mem::MaybeUninit<T>>::with_capacity(len);
        // SAFETY: the vector has room for `len` elements (including ZSTs), and
        // uninitialized storage is valid for MaybeUninit<T>. Its elements do
        // not drop T. The boxed slice retains the exact allocation layout.
        unsafe { values.set_len(len) };
        values.into_boxed_slice()
    }

    /// The elements, as a vector with room for them alone.
    pub fn into_vec(self) -> crate::vec::Vec<T> {
        let len = self.len();
        let ptr = Box::into_raw(self) as *mut T;
        unsafe { crate::vec::Vec::from_raw_parts(ptr, len, len) }
    }
}

impl<T> From<crate::vec::Vec<T>> for Box<[T]> {
    fn from(v: crate::vec::Vec<T>) -> Box<[T]> {
        v.into_boxed_slice()
    }
}

impl<T> From<T> for Box<T> {
    fn from(t: T) -> Box<T> {
        Box::new(t)
    }
}

impl<T: ?Sized + core::hash::Hash> core::hash::Hash for Box<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}
