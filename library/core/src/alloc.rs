//! Memory layouts, for allocators: `Layout`, a size and an alignment.

use crate::clone::Clone;
use crate::cmp::{Eq, PartialEq};
use crate::marker::Copy;
use crate::mem::{align_of, size_of};
use crate::result::Result::{self, Err, Ok};

/// A power of two, as rustc's `ptr::Alignment` is: every other value is
/// invalid, so enums around it may use them.
#[lang = "alignment"]
#[derive(Clone, Copy, PartialEq, Eq)]
struct Alignment(usize);

/// The size and alignment of a block of memory: what allocating it asks
/// for. The size, rounded up to the alignment, is at most `isize::MAX`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    size: usize,
    align: Alignment,
}

/// A size and alignment that make no layout.
#[derive(Clone, PartialEq, Eq)]
pub struct LayoutError;

impl crate::fmt::Debug for LayoutError {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        f.write_str("LayoutError")
    }
}

impl crate::fmt::Debug for Layout {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        f.write_str("Layout")
    }
}

impl Layout {
    /// `size` bytes aligned to `align`, a power of two, if their size
    /// rounded up to it fits an `isize`.
    pub const fn from_size_align(size: usize, align: usize) -> Result<Layout, LayoutError> {
        if align == 0 || align & (align - 1) != 0 {
            return Err(LayoutError);
        }
        if size > isize::MAX as usize - (align - 1) {
            return Err(LayoutError);
        }
        Ok(Layout {
            size,
            align: Alignment(align),
        })
    }

    /// `size` bytes aligned to `align`, unchecked.
    pub const unsafe fn from_size_align_unchecked(size: usize, align: usize) -> Layout {
        Layout {
            size,
            align: Alignment(align),
        }
    }

    /// The layout of a `T`.
    pub const fn new<T>() -> Layout {
        Layout {
            size: size_of::<T>(),
            align: Alignment(align_of::<T>()),
        }
    }

    /// The layout of the value `t` refers to.
    pub const fn for_value<T>(t: &T) -> Layout {
        let _ = t;
        Layout::new::<T>()
    }

    /// The layout of `n` `T`s in a row.
    pub const fn array<T>(n: usize) -> Result<Layout, LayoutError> {
        let size = size_of::<T>();
        let align = align_of::<T>();
        if size != 0 && n > (isize::MAX as usize - (align - 1)) / size {
            return Err(LayoutError);
        }
        Ok(Layout {
            size: size * n,
            align: Alignment(align),
        })
    }

    /// The size in bytes.
    pub const fn size(&self) -> usize {
        self.size
    }

    /// The alignment in bytes.
    pub const fn align(&self) -> usize {
        self.align.0
    }

    /// The size rounded up to the alignment.
    pub const fn pad_to_align(&self) -> Layout {
        let align = self.align.0;
        Layout {
            size: (self.size + align - 1) & !(align - 1),
            align: self.align,
        }
    }

    /// A pointer aligned for the layout that points to nothing.
    pub const fn dangling(&self) -> crate::ptr::NonNull<u8> {
        unsafe { crate::ptr::NonNull::new_unchecked(self.align.0 as *mut u8) }
    }
}
