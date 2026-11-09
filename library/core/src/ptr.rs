//! Raw pointers: making them (`null`, `NonNull`, `addr_of!` and
//! `addr_of_mut!`, as `&raw const` and `&raw mut` do), the methods of `*const
//! T` and `*mut T`, and reading, writing, copying and dropping what they
//! point to. As in rustc, these are `unsafe` where the caller must make
//! sure the pointer is valid. Copies of many values go through C's
//! `memmove` and `memcpy`.

use crate::clone::Clone;
use crate::cmp::{Eq, PartialEq};
use crate::marker::Copy;
use crate::mem::{ManuallyDrop, align_of, size_of};
use crate::option::Option::{self, None, Some};

unsafe extern "C" {
    fn memmove(dst: *mut u8, src: *const u8, n: usize) -> *mut u8;
    fn memcpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8;
    fn memset(dst: *mut u8, c: i32, n: usize) -> *mut u8;
}

/// `&raw const $place`: a `*const` pointer to a place, made without a
/// reference to it.
macro_rules! addr_of {
    ($place:expr) => {
        &raw const $place
    };
}

/// `&raw mut $place`: a `*mut` pointer to a place, made without a
/// reference to it (so allowed of a `static mut`).
macro_rules! addr_of_mut {
    ($place:expr) => {
        &raw mut $place
    };
}

pub use addr_of;
pub use addr_of_mut;

/// A null pointer, which points to nothing.
pub const fn null<T>() -> *const T {
    0 as *const T
}

/// A null mutable pointer.
pub const fn null_mut<T>() -> *mut T {
    0 as *mut T
}

/// Builds slice metadata without accessing `data` or making a reference.
pub const fn slice_from_raw_parts<T>(data: *const T, len: usize) -> *const [T] {
    crate::intrinsics::raw_slice_from_parts(data, len)
}

/// Builds mutable slice metadata without accessing `data` or making a reference.
pub const fn slice_from_raw_parts_mut<T>(data: *mut T, len: usize) -> *mut [T] {
    crate::intrinsics::raw_slice_from_parts_mut(data, len)
}

/// Whether two pointers have the same address and metadata.
pub fn eq<T: ?Sized>(a: *const T, b: *const T) -> bool {
    a == b
}

/// Hashes a thin pointer's address. Unsized pointees are not supported:
/// their metadata must also participate in pointer hashing.
pub fn hash<T, H: crate::hash::Hasher>(hashee: *const T, state: &mut H) {
    state.write_usize(hashee as usize);
}

/// Reads the value at `src` without moving it out: a bitwise copy,
/// which the caller makes sure is used and dropped only once.
pub const unsafe fn read<T>(src: *const T) -> T {
    unsafe { crate::intrinsics::read(src) }
}

/// Reads an initialized value without requiring `src` to be aligned.
/// The caller must provide readable memory for the entire value. Currently
/// only unsigned integer types are supported; other types are
/// rejected after substitution. Constant evaluation is not supported.
pub unsafe fn read_unaligned<T>(src: *const T) -> T {
    unsafe { crate::intrinsics::read_unaligned(src) }
}

/// Writes `value` at `dst`, without dropping what was there.
pub const unsafe fn write<T>(dst: *mut T, value: T) {
    unsafe { crate::intrinsics::write(dst, value) }
}

/// Writes an initialized value without requiring pointer alignment.
/// The destination must be valid for writes of the entire value. Existing
/// contents are overwritten without being dropped.
pub unsafe fn write_unaligned<T>(dst: *mut T, value: T) {
    let value = ManuallyDrop::new(value);
    // SAFETY: value is initialized local storage. Byte copying imposes no
    // alignment on dst, whose validity and ownership are the caller's contract.
    unsafe { copy_nonoverlapping(&*value as *const T as *const u8, dst as *mut u8, size_of::<T>()); }
}

/// Reads without allowing the compiler to remove or merge the access. The
/// pointer must be valid and aligned for an initialized value of T. This is
/// not atomic and does not synchronize threads. The current backend supports
/// scalar and same-layout single-field scalar wrapper types only.
pub unsafe fn read_volatile<T>(src:*const T)->T {
    // SAFETY: the caller guarantees aligned, initialized readable storage.
    unsafe { crate::intrinsics::read_volatile(src) }
}

/// Writes without allowing the compiler to remove or merge the access. The
/// pointer must be valid and aligned; overwritten contents are not dropped.
/// This is not atomic and does not synchronize threads. See read_volatile for
/// the currently supported bounded layouts.
pub unsafe fn write_volatile<T>(dst:*mut T,value:T) {
    // SAFETY: the caller guarantees aligned writable storage for the value.
    unsafe { crate::intrinsics::write_volatile(dst,value) }
}

/// Copies `count` values from `src` to `dst`, which may overlap.
pub unsafe fn copy<T>(src: *const T, dst: *mut T, count: usize) {
    unsafe {
        memmove(dst as *mut u8, src as *const u8, count * size_of::<T>());
    }
}

/// Copies `count` values from `src` to `dst`, which must not overlap.
pub unsafe fn copy_nonoverlapping<T>(src: *const T, dst: *mut T, count: usize) {
    unsafe {
        memcpy(dst as *mut u8, src as *const u8, count * size_of::<T>());
    }
}

/// Sets `count` values' bytes at `dst` to `value`.
pub unsafe fn write_bytes<T>(dst: *mut T, value: u8, count: usize) {
    unsafe {
        memset(dst as *mut u8, value as i32, count * size_of::<T>());
    }
}

/// Swaps the values at `x` and `y`, which may be the same.
pub unsafe fn swap<T>(x: *mut T, y: *mut T) {
    unsafe {
        let a = read(x);
        copy(y, x, 1);
        write(y, a);
    }
}

/// Swaps `count` initialized values between disjoint, aligned regions.
pub unsafe fn swap_nonoverlapping<T>(x: *mut T, y: *mut T, count: usize) {
    let mut i = 0;
    while i < count {
        unsafe {
            let value = read(x.add(i));
            copy_nonoverlapping(y.add(i), x.add(i), 1);
            write(y.add(i), value);
        }
        i += 1;
    }
}

/// Puts `src` at `dst`, giving back what was there.
pub unsafe fn replace<T>(dst: *mut T, src: T) -> T {
    unsafe {
        let old = read(dst);
        write(dst, src);
        old
    }
}

/// Drops the value at `to_drop`, which is then not to be used again.
#[lang = "drop_in_place"]
pub unsafe fn drop_in_place<T: ?Sized>(to_drop: *mut T) {
    unsafe { crate::intrinsics::drop_in_place(to_drop) }
}

impl<T: ?Sized> *const T {
    /// Replaces the data address while retaining pointer metadata.
    pub fn with_addr(self,address:usize)->Self {
        crate::intrinsics::ptr_with_addr(self as *const T,address) as *const T
    }
    /// Maps the data address while retaining pointer metadata.
    pub fn map_addr<F:FnOnce(usize)->usize>(self,map:F)->Self {self.with_addr(map(self.addr()))}

    /// Whether the pointer is null.
    pub fn is_null(self) -> bool {
        self as *const u8 as usize == 0
    }

    /// The pointer as one to a `U`.
    pub const fn cast<U>(self) -> *const U {
        self as *const U
    }

    /// The pointer as a mutable one.
    pub const fn cast_mut(self) -> *mut T {
        self as *mut T
    }

    /// The address.
    pub fn addr(self) -> usize {
        self as *const u8 as usize
    }

    /// A reference to the value, or `None` for a null pointer.
    pub unsafe fn as_ref<'a>(self) -> Option<&'a T> {
        if self.is_null() {
            None
        } else {
            unsafe { Some(&*self) }
        }
    }
}

impl<T> *const T {
    /// Offsets the data address by count bytes.
    /// # Safety
    /// The displacement must fit isize and remain in the same allocation.
    pub const unsafe fn byte_add(self,count:usize)->*const T {
        // SAFETY: the caller supplies byte-allocation bounds.
        unsafe {(self as *const u8).add(count) as *const T}
    }
    /// Offsets the data address by count bytes with wrapping arithmetic.
    pub fn wrapping_byte_add(self,count:usize)->*const T {
        (self as *const u8).wrapping_add(count) as *const T
    }

    /// Offsets the data address by count bytes.
    /// # Safety
    /// The displacement must fit isize and remain in the same allocation.
    pub const unsafe fn byte_sub(self,count:usize)->*const T {
        // SAFETY: the caller supplies byte-allocation bounds.
        unsafe {(self as *const u8).sub(count) as *const T}
    }
    /// Offsets the data address by count bytes with wrapping arithmetic.
    pub fn wrapping_byte_sub(self,count:usize)->*const T {
        (self as *const u8).wrapping_sub(count) as *const T
    }

    /// The nonnegative element distance from origin.
    /// # Safety
    /// Both pointers must belong to the same allocation, with self at or after
    /// origin and an isize-sized, whole-element displacement. T must not be ZST.
    pub unsafe fn offset_from_unsigned(self,origin:*const T)->usize {
        // SAFETY: the caller supplies the shared allocation and element bounds.
        let distance=unsafe {self.offset_from(origin)};
        assert!(distance>=0,"unsigned pointer distance must be nonnegative");
        distance as usize
    }

    /// The element offset needed to reach `align`, or `usize::MAX` if
    /// impossible. `align` must be a power of two. This checks alignment
    /// only, not allocation bounds or whether memory can be accessed.
    /// Constant evaluation is not supported.
    #[track_caller]
    pub fn align_offset(self, align: usize) -> usize {
        assert!(align.is_power_of_two(), "align_offset: align is not a power-of-two");
        alignment_offset(self as usize, size_of::<T>(), align)
    }

    /// The pointer `count` values on.
    ///
    /// # Safety
    /// The byte displacement must fit in isize and stay within the same
    /// allocation or at its end. Zero-sized pointees do not move the address.
    pub const unsafe fn add(self, count: usize) -> *const T {
        // SAFETY: the caller guarantees the offset stays within the allocation.
        unsafe { crate::intrinsics::ptr_add(self, count) }
    }

    /// The pointer `count` values back.
    ///
    /// # Safety
    /// The byte displacement must fit in isize and stay within the same
    /// allocation or at its end. Zero-sized pointees do not move the address.
    pub const unsafe fn sub(self, count: usize) -> *const T {
        // SAFETY: the caller guarantees the offset stays within the allocation.
        unsafe { crate::intrinsics::ptr_sub(self, count) }
    }

    /// The pointer `count` values on, or back if it is negative.
    ///
    /// # Safety
    /// The byte displacement must fit in isize and stay within the same
    /// allocation or at its end. Zero-sized pointees do not move the address.
    pub const unsafe fn offset(self, count: isize) -> *const T {
        // SAFETY: the caller guarantees the offset stays within the allocation.
        unsafe { crate::intrinsics::ptr_offset(self, count) }
    }

    /// [`Self::add`], for any `count`.
    pub fn wrapping_add(self, count: usize) -> *const T {
        (self as usize).wrapping_add(count.wrapping_mul(size_of::<T>())) as *const T
    }

    /// [`Self::sub`], for any `count`.
    pub fn wrapping_sub(self, count: usize) -> *const T {
        (self as usize).wrapping_sub(count.wrapping_mul(size_of::<T>())) as *const T
    }

    /// Offsets by `count` elements, wrapping the address space.
    /// The result need not be in bounds; accessing it still requires validity.
    pub fn wrapping_offset(self, count: isize) -> *const T {
        self.wrapping_add(count as usize)
    }

    /// How many values `origin` is before the pointer.
    pub unsafe fn offset_from(self, origin: *const T) -> isize {
        ((self as usize).wrapping_sub(origin as usize) as isize) / (size_of::<T>() as isize)
    }

    /// Reads the value the pointer points to.
    pub const unsafe fn read(self) -> T {
        unsafe { read(self) }
    }

    /// Reads with volatile access semantics; see [`read_volatile`].
    pub unsafe fn read_volatile(self)->T {
        // SAFETY: the caller guarantees this pointer's validity and alignment.
        unsafe { read_volatile(self) }
    }

    /// Reads an initialized value without requiring pointer alignment.
    /// See [`read_unaligned`] for the supported types and safety contract.
    pub unsafe fn read_unaligned(self) -> T {
        unsafe { read_unaligned(self) }
    }

    /// Copies `count` values from the pointer to `dest`.
    pub unsafe fn copy_to(self, dest: *mut T, count: usize) {
        unsafe { copy(self, dest, count) }
    }

    /// Copies `count` values from the pointer to `dest`, which must not
    /// overlap.
    pub unsafe fn copy_to_nonoverlapping(self, dest: *mut T, count: usize) {
        unsafe { copy_nonoverlapping(self, dest, count) }
    }

}

impl<T: ?Sized> *mut T {
    /// Drops the value the pointer points to.
    /// # Safety
    /// The pointee must be valid for dropping, as `ptr::drop_in_place` requires.
    pub unsafe fn drop_in_place(self) {
        unsafe { drop_in_place(self) }
    }

    /// Replaces the data address while retaining pointer metadata.
    pub fn with_addr(self,address:usize)->Self {
        crate::intrinsics::ptr_with_addr(self as *const T,address) as *mut T
    }
    /// Maps the data address while retaining pointer metadata.
    pub fn map_addr<F:FnOnce(usize)->usize>(self,map:F)->Self {self.with_addr(map(self.addr()))}

    /// Whether the pointer is null.
    pub fn is_null(self) -> bool {
        self as *const u8 as usize == 0
    }

    /// The pointer as one to a `U`.
    pub const fn cast<U>(self) -> *mut U {
        self as *mut U
    }

    /// The pointer as a shared one.
    pub const fn cast_const(self) -> *const T {
        self as *const T
    }

    /// The address.
    pub fn addr(self) -> usize {
        self as *const u8 as usize
    }

    /// A reference to the value, or `None` for a null pointer.
    pub unsafe fn as_ref<'a>(self) -> Option<&'a T> {
        if self.is_null() {
            None
        } else {
            unsafe { Some(&*self) }
        }
    }

    /// A mutable reference to the value, or `None` for a null pointer.
    pub unsafe fn as_mut<'a>(self) -> Option<&'a mut T> {
        if self.is_null() {
            None
        } else {
            unsafe { Some(&mut *self) }
        }
    }
}

impl<T> *mut T {
    /// Offsets the data address by count bytes.
    /// # Safety
    /// The displacement must fit isize and remain in the same allocation.
    pub const unsafe fn byte_add(self,count:usize)->*mut T {
        // SAFETY: the caller supplies byte-allocation bounds.
        unsafe {(self as *mut u8).add(count) as *mut T}
    }
    /// Offsets the data address by count bytes with wrapping arithmetic.
    pub fn wrapping_byte_add(self,count:usize)->*mut T {
        (self as *mut u8).wrapping_add(count) as *mut T
    }

    /// Offsets the data address by count bytes.
    /// # Safety
    /// The displacement must fit isize and remain in the same allocation.
    pub const unsafe fn byte_sub(self,count:usize)->*mut T {
        // SAFETY: the caller supplies byte-allocation bounds.
        unsafe {(self as *mut u8).sub(count) as *mut T}
    }
    /// Offsets the data address by count bytes with wrapping arithmetic.
    pub fn wrapping_byte_sub(self,count:usize)->*mut T {
        (self as *mut u8).wrapping_sub(count) as *mut T
    }

    /// The nonnegative element distance from origin.
    /// # Safety
    /// Both pointers must belong to the same allocation, with self at or after
    /// origin and an isize-sized, whole-element displacement. T must not be ZST.
    pub unsafe fn offset_from_unsigned(self,origin:*const T)->usize {
        // SAFETY: the caller supplies the shared allocation and element bounds.
        let distance=unsafe {self.offset_from(origin)};
        assert!(distance>=0,"unsigned pointer distance must be nonnegative");
        distance as usize
    }

    /// The element offset needed to reach `align`, or `usize::MAX` if
    /// impossible. See the shared pointer method for the contract.
    #[track_caller]
    pub fn align_offset(self, align: usize) -> usize {
        (self as *const T).align_offset(align)
    }

    /// The pointer `count` values on.
    ///
    /// # Safety
    /// The byte displacement must fit in isize and stay within the same
    /// allocation or at its end. Zero-sized pointees do not move the address.
    pub const unsafe fn add(self, count: usize) -> *mut T {
        // SAFETY: the caller guarantees the offset stays within the allocation.
        unsafe { crate::intrinsics::ptr_add(self as *const T, count) as *mut T }
    }

    /// The pointer `count` values back.
    ///
    /// # Safety
    /// The byte displacement must fit in isize and stay within the same
    /// allocation or at its end. Zero-sized pointees do not move the address.
    pub const unsafe fn sub(self, count: usize) -> *mut T {
        // SAFETY: the caller guarantees the offset stays within the allocation.
        unsafe { crate::intrinsics::ptr_sub(self as *const T, count) as *mut T }
    }

    /// The pointer `count` values on, or back if it is negative.
    ///
    /// # Safety
    /// The byte displacement must fit in isize and stay within the same
    /// allocation or at its end. Zero-sized pointees do not move the address.
    pub const unsafe fn offset(self, count: isize) -> *mut T {
        // SAFETY: the caller guarantees the offset stays within the allocation.
        unsafe { crate::intrinsics::ptr_offset(self as *const T, count) as *mut T }
    }

    /// [`Self::add`], for any `count`.
    pub fn wrapping_add(self, count: usize) -> *mut T {
        (self as usize).wrapping_add(count.wrapping_mul(size_of::<T>())) as *mut T
    }

    /// [`Self::sub`], for any `count`.
    pub fn wrapping_sub(self, count: usize) -> *mut T {
        (self as usize).wrapping_sub(count.wrapping_mul(size_of::<T>())) as *mut T
    }

    /// Offsets by `count` elements, wrapping the address space.
    /// The result need not be in bounds; accessing it still requires validity.
    pub fn wrapping_offset(self, count: isize) -> *mut T {
        self.wrapping_add(count as usize)
    }

    /// How many values `origin` is before the pointer.
    pub unsafe fn offset_from(self, origin: *const T) -> isize {
        ((self as usize).wrapping_sub(origin as usize) as isize) / (size_of::<T>() as isize)
    }

    /// Reads the value the pointer points to.
    pub const unsafe fn read(self) -> T {
        unsafe { read(self) }
    }

    /// Writes `value` where the pointer points, without dropping what
    /// was there.
    pub const unsafe fn write(self, value: T) {
        unsafe { write(self, value) }
    }

    /// Writes with volatile access semantics; see [`write_volatile`].
    pub unsafe fn write_volatile(self,value:T) {
        // SAFETY: the caller guarantees this pointer's validity and alignment.
        unsafe { write_volatile(self,value) }
    }

    /// Writes an initialized value without requiring pointer alignment.
    /// See [`write_unaligned`] for the validity and ownership contract.
    pub unsafe fn write_unaligned(self, value: T) {
        unsafe { write_unaligned(self, value) }
    }

    /// Puts `src` where the pointer points, giving back what was there.
    pub unsafe fn replace(self, src: T) -> T {
        unsafe { replace(self, src) }
    }

    /// Swaps the values at the pointer and `with`.
    pub unsafe fn swap(self, with: *mut T) {
        unsafe { swap(self, with) }
    }


    /// Sets `count` values' bytes to `value`.
    pub unsafe fn write_bytes(self, value: u8, count: usize) {
        unsafe { write_bytes(self, value, count) }
    }

    /// Reads with volatile access semantics; see [`read_volatile`].
    pub unsafe fn read_volatile(self)->T {
        // SAFETY: the caller guarantees this pointer's validity and alignment.
        unsafe { read_volatile(self) }
    }

    /// Reads an initialized value without requiring pointer alignment.
    /// See [`read_unaligned`] for the supported types and safety contract.
    pub unsafe fn read_unaligned(self) -> T {
        unsafe { read_unaligned(self) }
    }

    /// Copies `count` values from the pointer to `dest`.
    pub unsafe fn copy_to(self, dest: *mut T, count: usize) {
        unsafe { copy(self, dest, count) }
    }

    /// Copies `count` values from the pointer to `dest`, which must not
    /// overlap.
    pub unsafe fn copy_to_nonoverlapping(self, dest: *mut T, count: usize) {
        unsafe { copy_nonoverlapping(self, dest, count) }
    }

    /// Copies `count` values from `src` to the pointer.
    pub unsafe fn copy_from(self, src: *const T, count: usize) {
        unsafe { copy(src, self, count) }
    }

    /// Copies `count` values from `src` to the pointer, which must not
    /// overlap.
    pub unsafe fn copy_from_nonoverlapping(self, src: *const T, count: usize) {
        unsafe { copy_nonoverlapping(src, self, count) }
    }

}

// Solve address + stride * offset = 0 (mod align), where align is a
// nonzero power of two. All arithmetic concerns addresses only; no
// pointer is constructed or accessed here.
fn alignment_offset(address: usize, stride: usize, align: usize) -> usize {
    if address & (align - 1) == 0 {
        return 0;
    }
    if stride == 0 {
        return usize::MAX;
    }
    let stride_bits = stride.trailing_zeros();
    let align_bits = align.trailing_zeros();
    let shift = if stride_bits < align_bits { stride_bits } else { align_bits };
    if address & ((1usize << shift) - 1) != 0 {
        return usize::MAX;
    }
    let mask = (align >> shift) - 1;
    let odd_stride = stride >> shift;
    let distance = 0usize.wrapping_sub(address >> shift);
    if odd_stride == 1 {
        return distance & mask;
    }
    // An odd number has inverse 1 modulo 2. Newton lifting doubles
    // the number of correct bits each time: y <- y * (2 - x*y).
    // At most six iterations are needed on a 64-bit target, regardless
    // of the address, stride or requested alignment.
    let mut inverse = 1usize;
    let mut bits = 1u32;
    while bits < align_bits - shift {
        inverse = inverse.wrapping_mul(2usize.wrapping_sub(odd_stride.wrapping_mul(inverse)));
        bits *= 2;
    }
    distance.wrapping_mul(inverse) & mask
}

/// A `*mut T` that is never null, so that `Option<NonNull<T>>` is one
/// pointer, as rustc lays it out (`#[rustc_layout_scalar_valid_range_start(1)]`
/// there; its `lang` name here).
#[lang = "non_null"]
pub struct NonNull<T: ?Sized> {
    pointer: *const T,
}

impl<T: ?Sized> NonNull<T> {
    /// Replaces the nonzero data address, retaining pointer metadata.
    pub fn with_addr(self,address:crate::num::NonZeroUsize)->Self {
        // SAFETY: the new address is nonzero; validity is required only on access.
        unsafe {Self::new_unchecked(self.as_ptr().with_addr(address.get()))}
    }
    /// Maps the nonzero data address, retaining pointer metadata.
    pub fn map_addr<F:FnOnce(crate::num::NonZeroUsize)->crate::num::NonZeroUsize>(self,map:F)->Self {self.with_addr(map(self.addr()))}

    /// The nonzero data address, without metadata.
    pub fn addr(self)->crate::num::NonZeroUsize {
        // SAFETY: NonNull guarantees its data address is nonzero.
        unsafe {crate::num::NonZeroUsize::new_unchecked(self.pointer.addr())}
    }

    /// `ptr`, which must not be null.
    pub const unsafe fn new_unchecked(ptr: *mut T) -> NonNull<T> {
        NonNull {
            pointer: ptr as *const T,
        }
    }

    /// `ptr`, if it is not null.
    pub fn new(ptr: *mut T) -> Option<NonNull<T>> {
        if ptr.is_null() {
            None
        } else {
            Some(NonNull {
                pointer: ptr as *const T,
            })
        }
    }

    /// The pointer.
    #[inline(always)]
    pub const fn as_ptr(self) -> *mut T {
        self.pointer as *mut T
    }

    /// A reference to the value it points to, which must be valid for
    /// as long as the reference is used.
    pub unsafe fn as_ref<'a>(&self) -> &'a T {
        unsafe { &*self.pointer }
    }

    /// A mutable reference to the value it points to.
    pub unsafe fn as_mut<'a>(&mut self) -> &'a mut T {
        unsafe { &mut *(self.pointer as *mut T) }
    }

    /// The pointer as one to a `U`.
    pub const fn cast<U>(self) -> NonNull<U> {
        NonNull {
            pointer: self.pointer as *const U,
        }
    }
}

impl<T> NonNull<T> {
    /// Offsets the pointer by count bytes.
    /// # Safety
    /// The displacement must fit isize and stay in the same allocation.
    pub const unsafe fn byte_add(self,count:usize)->Self {
        // SAFETY: a valid in-allocation offset preserves non-nullness.
        unsafe {Self::new_unchecked(self.as_ptr().byte_add(count))}
    }

    /// Offsets the pointer by count bytes.
    /// # Safety
    /// The displacement must fit isize and stay in the same allocation.
    pub const unsafe fn byte_sub(self,count:usize)->Self {
        // SAFETY: a valid in-allocation offset preserves non-nullness.
        unsafe {Self::new_unchecked(self.as_ptr().byte_sub(count))}
    }

    /// The nonnegative element distance from origin.
    /// # Safety
    /// The pointers must share an allocation, self must not precede origin,
    /// and the displacement must fit isize and whole nonzero-sized elements.
    pub unsafe fn offset_from_unsigned(self,origin:Self)->usize {
        // SAFETY: forwarded caller guarantees.
        unsafe {self.pointer.offset_from_unsigned(origin.pointer)}
    }
    /// Replaces the initialized value without dropping the old value.
    /// # Safety
    /// The pointer must be valid and aligned for reading and writing T.
    pub unsafe fn replace(self,value:T)->T {
        // SAFETY: forwarded caller guarantees.
        unsafe {replace(self.as_ptr(),value)}
    }
    /// Copies count elements, permitting overlap.
    /// # Safety
    /// Both ranges must be valid and aligned for count elements.
    pub unsafe fn copy_to(self,dest:Self,count:usize) {
        // SAFETY: forwarded caller guarantees.
        unsafe {copy(self.pointer,dest.as_ptr(),count);}
    }
    /// Copies count elements between disjoint ranges.
    /// # Safety
    /// Both ranges must be valid, aligned and disjoint for count elements.
    pub unsafe fn copy_to_nonoverlapping(self,dest:Self,count:usize) {
        // SAFETY: forwarded caller guarantees.
        unsafe {copy_nonoverlapping(self.pointer,dest.as_ptr(),count);}
    }
    /// Fills count elements with the specified byte pattern.
    /// # Safety
    /// The pointer must be valid and aligned for writing count elements. The
    /// pattern must be a valid T before a resulting element is read.
    pub unsafe fn write_bytes(self,value:u8,count:usize) {
        // SAFETY: forwarded caller guarantees.
        unsafe {write_bytes(self.as_ptr(),value,count);}
    }

    /// A pointer aligned for a `T` that points to nothing: what an empty
    /// collection holds.
    pub const fn dangling() -> NonNull<T> {
        NonNull {
            pointer: align_of::<T>() as *const T,
        }
    }

    /// The pointer `count` values on.
    ///
    /// # Safety
    /// The byte displacement must fit in isize and stay within the same
    /// allocation or at its end. Zero-sized pointees do not move the address.
    pub const unsafe fn add(self, count: usize) -> NonNull<T> {
        NonNull {
            pointer: unsafe { self.pointer.add(count) },
        }
    }

    /// Reads the value it points to.
    pub const unsafe fn read(self) -> T {
        unsafe { read(self.pointer) }
    }

    /// Writes `value` where it points, without dropping what was there.
    pub const unsafe fn write(self, value: T) {
        unsafe { write(self.pointer as *mut T, value) }
    }
}

impl<T: ?Sized> Clone for NonNull<T> {
    fn clone(&self) -> NonNull<T> {
        *self
    }
}

impl<T: ?Sized> Copy for NonNull<T> {}

impl<T: ?Sized> crate::convert::From<&T> for NonNull<T> {
    fn from(value: &T) -> NonNull<T> {
        NonNull {
            pointer: value as *const T,
        }
    }
}

impl<T: ?Sized> crate::convert::From<&mut T> for NonNull<T> {
    fn from(value: &mut T) -> NonNull<T> {
        NonNull {
            pointer: value as *mut T as *const T,
        }
    }
}

impl<T> *const [T] {
    pub const fn len(self) -> usize {
        crate::intrinsics::raw_slice_len(self)
    }
}

impl<T> *mut [T] {
    pub const fn len(self) -> usize {
        crate::intrinsics::raw_slice_len(self as *const [T])
    }
}

impl<T> NonNull<[T]> {
    /// Adds slice-length metadata without accessing the allocation.
    pub const fn slice_from_raw_parts(data: NonNull<T>, len: usize) -> Self {
        NonNull { pointer: slice_from_raw_parts_mut(data.as_ptr(), len) }
    }
    /// The number of elements in the slice, without accessing its contents.
    pub const fn len(self) -> usize {
        crate::intrinsics::raw_slice_len(self.pointer)
    }
}

impl<T: ?Sized> PartialEq for NonNull<T> {
    fn eq(&self, other: &NonNull<T>) -> bool {
        self.pointer as *const u8 as usize == other.pointer as *const u8 as usize
    }
}

impl<T: ?Sized> Eq for NonNull<T> {}

impl<T> crate::cmp::PartialOrd for NonNull<T> {
    fn partial_cmp(&self, other: &Self) -> Option<crate::cmp::Ordering> {
        Some(crate::cmp::Ord::cmp(self, other))
    }
}
impl<T> crate::cmp::Ord for NonNull<T> {
    fn cmp(&self, other: &Self) -> crate::cmp::Ordering {
        (self.pointer as usize).cmp(&(other.pointer as usize))
    }
}
impl<T: ?Sized> crate::fmt::Pointer for NonNull<T> {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        crate::fmt::Pointer::fmt(&self.pointer, f)
    }
}

impl<T: ?Sized> crate::fmt::Debug for NonNull<T> {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        crate::fmt::Debug::fmt(&self.pointer, f)
    }
}

/// Converts a reference to a raw pointer without changing its address.
pub const fn from_ref<T: ?Sized>(r: &T) -> *const T {
    r as *const T
}

/// Converts an exclusive reference to a raw pointer without changing its address.
pub const fn from_mut<T: ?Sized>(r: &mut T) -> *mut T {
    r as *mut T
}
