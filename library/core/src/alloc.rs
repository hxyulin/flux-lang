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

/// The former name of `LayoutError`.
#[deprecated(since = "1.28.0", note = "use LayoutError")]
pub type LayoutErr = LayoutError;

impl crate::fmt::Debug for LayoutError {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        f.write_str("LayoutError")
    }
}

impl crate::fmt::Debug for Layout {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        f.debug_struct("Layout")
            .field("size", &self.size)
            .field("align", &self.align)
            .finish()
    }
}

impl crate::fmt::Debug for Alignment {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        let mut shift = 0;
        let mut align = self.0;
        while align > 1 {
            align >>= 1;
            shift += 1;
        }
        write!(f, "{} (1 << {})", self.0, shift)
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
    pub const fn for_value<T: ?Sized>(t: &T) -> Layout {
        Layout {
            size: crate::mem::size_of_val(t),
            align: Alignment(crate::mem::align_of_val(t)),
        }
    }

    /// The layout described by a raw pointer's type and metadata.
    ///
    /// # Safety
    /// Slice lengths and trait-object vtables must describe a valid layout.
    /// The payload need not be initialized or borrowed as a T.
    pub const unsafe fn for_value_raw<T: ?Sized>(ptr: *const T) -> Layout {
        Layout {
            // SAFETY: the caller supplies valid layout metadata. These
            // intrinsics inspect that metadata without accessing T.
            size: unsafe { crate::intrinsics::size_of_val(ptr) },
            // SAFETY: the same valid metadata gives alignment without a T.
            align: Alignment(unsafe { crate::intrinsics::align_of_val(ptr) }),
        }
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
