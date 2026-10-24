//! Raw pointers: making them (`null`, `NonNull`, `addr_of!` and
//! `addr_of_mut!`, as `&raw const` and `&raw mut` do), the methods of `*const
//! T` and `*mut T`, and reading, writing, copying and dropping what they
//! point to. As in rustc, these are `unsafe` where the caller must make
//! sure the pointer is valid. Copies of many values go through C's
//! `memmove` and `memcpy`.

use crate::clone::Clone;
use crate::cmp::{Eq, PartialEq};
use crate::marker::Copy;
use crate::mem::{align_of, size_of};
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

/// Whether two pointers are equal: the same address.
pub fn eq<T>(a: *const T, b: *const T) -> bool {
    a as usize == b as usize
}

/// Hashes a thin pointer's address. Unsized pointees are not supported:
/// their metadata must also participate in pointer hashing.
pub fn hash<T, H: crate::hash::Hasher>(hashee: *const T, state: &mut H) {
    state.write_usize(hashee as usize);
}

/// Reads the value at `src` without moving it out: a bitwise copy,
/// which the caller makes sure is used and dropped only once.
pub unsafe fn read<T>(src: *const T) -> T {
    unsafe { crate::intrinsics::read(&*src) }
}

/// Reads an initialized value without requiring `src` to be aligned.
/// The caller must provide readable memory for the entire value. Currently
/// only `u8`, `u16`, `u32`, `u64` and `usize` are supported; other types are
/// rejected after substitution. Constant evaluation is not supported.
pub unsafe fn read_unaligned<T>(src: *const T) -> T {
    unsafe { crate::intrinsics::read_unaligned(src) }
}

/// Writes `value` at `dst`, without dropping what was there.
pub unsafe fn write<T>(dst: *mut T, value: T) {
    unsafe { crate::intrinsics::write(&mut *dst, value) }
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
    pub unsafe fn add(self, count: usize) -> *const T {
        (self as usize).wrapping_add(count.wrapping_mul(size_of::<T>())) as *const T
    }

    /// The pointer `count` values back.
    pub unsafe fn sub(self, count: usize) -> *const T {
        (self as usize).wrapping_sub(count.wrapping_mul(size_of::<T>())) as *const T
    }

    /// The pointer `count` values on, or back if it is negative.
    pub unsafe fn offset(self, count: isize) -> *const T {
        (self as usize).wrapping_add((count as usize).wrapping_mul(size_of::<T>())) as *const T
    }

    /// [`Self::add`], for any `count`.
    pub fn wrapping_add(self, count: usize) -> *const T {
        (self as usize).wrapping_add(count.wrapping_mul(size_of::<T>())) as *const T
    }

    /// [`Self::sub`], for any `count`.
    pub fn wrapping_sub(self, count: usize) -> *const T {
        (self as usize).wrapping_sub(count.wrapping_mul(size_of::<T>())) as *const T
    }

    /// How many values `origin` is before the pointer.
    pub unsafe fn offset_from(self, origin: *const T) -> isize {
        ((self as usize).wrapping_sub(origin as usize) as isize) / (size_of::<T>() as isize)
    }

    /// Reads the value the pointer points to.
    pub unsafe fn read(self) -> T {
        unsafe { read(self) }
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
    /// The element offset needed to reach `align`, or `usize::MAX` if
    /// impossible. See the shared pointer method for the contract.
    #[track_caller]
    pub fn align_offset(self, align: usize) -> usize {
        (self as *const T).align_offset(align)
    }

    /// The pointer `count` values on.
    pub unsafe fn add(self, count: usize) -> *mut T {
        (self as usize).wrapping_add(count.wrapping_mul(size_of::<T>())) as *mut T
    }

    /// The pointer `count` values back.
    pub unsafe fn sub(self, count: usize) -> *mut T {
        (self as usize).wrapping_sub(count.wrapping_mul(size_of::<T>())) as *mut T
    }

    /// The pointer `count` values on, or back if it is negative.
    pub unsafe fn offset(self, count: isize) -> *mut T {
        (self as usize).wrapping_add((count as usize).wrapping_mul(size_of::<T>())) as *mut T
    }

    /// [`Self::add`], for any `count`.
    pub fn wrapping_add(self, count: usize) -> *mut T {
        (self as usize).wrapping_add(count.wrapping_mul(size_of::<T>())) as *mut T
    }

    /// [`Self::sub`], for any `count`.
    pub fn wrapping_sub(self, count: usize) -> *mut T {
        (self as usize).wrapping_sub(count.wrapping_mul(size_of::<T>())) as *mut T
    }

    /// How many values `origin` is before the pointer.
    pub unsafe fn offset_from(self, origin: *const T) -> isize {
        ((self as usize).wrapping_sub(origin as usize) as isize) / (size_of::<T>() as isize)
    }

    /// Reads the value the pointer points to.
    pub unsafe fn read(self) -> T {
        unsafe { read(self) }
    }

    /// Writes `value` where the pointer points, without dropping what
    /// was there.
    pub unsafe fn write(self, value: T) {
        unsafe { write(self, value) }
    }

    /// Puts `src` where the pointer points, giving back what was there.
    pub unsafe fn replace(self, src: T) -> T {
        unsafe { replace(self, src) }
    }

    /// Swaps the values at the pointer and `with`.
    pub unsafe fn swap(self, with: *mut T) {
        unsafe { swap(self, with) }
    }

    /// Drops the value the pointer points to.
    pub unsafe fn drop_in_place(self) {
        unsafe { drop_in_place(self) }
    }

    /// Sets `count` values' bytes to `value`.
    pub unsafe fn write_bytes(self, value: u8, count: usize) {
        unsafe { write_bytes(self, value, count) }
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
    /// A pointer aligned for a `T` that points to nothing: what an empty
    /// collection holds.
    pub const fn dangling() -> NonNull<T> {
        NonNull {
            pointer: align_of::<T>() as *const T,
        }
    }

    /// The pointer `count` values on.
    pub unsafe fn add(self, count: usize) -> NonNull<T> {
        NonNull {
            pointer: unsafe { self.pointer.add(count) },
        }
    }

    /// Reads the value it points to.
    pub unsafe fn read(self) -> T {
        unsafe { read(self.pointer) }
    }

    /// Writes `value` where it points, without dropping what was there.
    pub unsafe fn write(self, value: T) {
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

impl<T> NonNull<[T]> {
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
