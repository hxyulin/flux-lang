//! Memory layouts, for allocators: `Layout`, a size and an alignment.

use crate::clone::Clone;
use crate::cmp::{Eq, PartialEq};
use crate::marker::Copy;
use crate::mem::{align_of, size_of};
use crate::result::Result::{self, Err, Ok};

/// An allocator for nonzero memory blocks described by a `Layout`.
///
/// # Safety
/// Implementations must return null or a fresh block matching the requested
/// layout, honor the allocation ownership contracts, and never unwind.
pub unsafe trait GlobalAlloc {
    /// Allocates a block, or returns null on failure.
    ///
    /// # Safety
    /// `layout.size()` must be nonzero.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8;

    /// Releases a block previously returned by this allocator.
    ///
    /// # Safety
    /// `pointer` must denote a currently allocated block with this layout.
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout);

    /// Allocates a block whose bytes are all zero, or returns null.
    ///
    /// # Safety
    /// `layout.size()` must be nonzero.
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies a nonzero valid allocation layout.
        let pointer = unsafe { self.alloc(layout) };
        if !pointer.is_null() {
            // SAFETY: successful allocation provides layout.size() writable bytes.
            unsafe { crate::ptr::write_bytes(pointer, 0, layout.size()); }
        }
        pointer
    }

    /// Resizes a block, preserving its prefix. Failure keeps the old block live.
    ///
    /// # Safety
    /// `pointer` must denote this allocator's live block with `layout`.
    /// `new_size` must be nonzero and form a valid layout with the same alignment.
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees the new size and alignment form a layout.
        let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
        // SAFETY: new_layout has the caller's nonzero requested size.
        let replacement = unsafe { self.alloc(new_layout) };
        if !replacement.is_null() {
            let count = if layout.size() < new_size { layout.size() } else { new_size };
            // SAFETY: both blocks are live, disjoint and at least count bytes long.
            // Ownership of the old block ends only after successful copying.
            unsafe {
                crate::ptr::copy_nonoverlapping(pointer, replacement, count);
                self.dealloc(pointer, layout);
            }
        }
        replacement
    }
}

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

impl Layout {
    /// A layout for this value followed by `next`, and the offset of `next`.
    pub const fn extend(&self, next: Layout) -> Result<(Layout, usize), LayoutError> {
        let new_align = if self.align.0 > next.align.0 { self.align.0 } else { next.align.0 };
        let offset = (self.size + (next.align.0 - 1)) & !(next.align.0 - 1);
        let new_size = match offset.checked_add(next.size) { Some(size) => size, None => return Err(LayoutError) };
        match Layout::from_size_align(new_size, new_align) {
            Ok(layout) => Ok((layout, offset)),
            Err(error) => Err(error),
        }
    }
    /// The same size with at least `align` alignment.
    pub const fn align_to(&self, align: usize) -> Result<Layout, LayoutError> {
        let align = if self.align.0 > align { self.align.0 } else { align };
        Layout::from_size_align(self.size, align)
    }
}
