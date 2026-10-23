//! Operations the compiler implements, which `core` builds its arrays'
//! and slices' methods from. A call of one is replaced with what it does
//! once its types are known, when generic code is monomorphized, so
//! their bodies never run; their signatures are what the borrow checker
//! sees.

/// A bitwise copy of `*src`, which need not be `Copy`: the caller makes
/// sure that only one of the two is used and dropped.
#[rustc_intrinsic]
pub(crate) unsafe fn read<T>(src: &T) -> T {
    loop {}
}

/// Stores `value` in `*dst` without dropping what was there.
#[rustc_intrinsic]
pub(crate) unsafe fn write<T>(dst: &mut T, value: T) {
    loop {}
}

/// A value of type `T` that is not there: memory to write each part of
/// before it is used (`array::from_fn`'s array).
#[rustc_intrinsic]
pub(crate) const unsafe fn uninit<T>() -> T {
    loop {}
}

/// Whether `T` is `Copy`: what rustc's specialization of arrays'
/// `Clone` asks.
#[rustc_intrinsic]
pub(crate) fn is_copy<T>() -> bool {
    loop {}
}

/// The size of a `T` in bytes, as rustc lays it out.
#[rustc_intrinsic]
pub(crate) const fn size_of<T>() -> usize {
    loop {}
}

/// The alignment of a `T` in bytes.
#[rustc_intrinsic]
pub(crate) const fn align_of<T>() -> usize {
    loop {}
}

/// The size of the value `ptr` points to: `size_of::<T>()` for a sized
/// `T`, the elements' for a slice or `str`, the vtable's for a trait
/// object.
#[rustc_intrinsic]
pub(crate) const unsafe fn size_of_val<T: ?Sized>(ptr: *const T) -> usize {
    loop {}
}

/// The alignment of the value `ptr` points to.
#[rustc_intrinsic]
pub(crate) const unsafe fn align_of_val<T: ?Sized>(ptr: *const T) -> usize {
    loop {}
}

/// Drops the value `ptr` points to, in place: its drop glue, a slice's
/// elements in order, or through a trait object's vtable.
#[rustc_intrinsic]
pub(crate) unsafe fn drop_in_place<T: ?Sized>(ptr: *mut T) {
    loop {}
}

/// Whether dropping a `T` runs code.
#[rustc_intrinsic]
pub(crate) const fn needs_drop<T>() -> bool {
    loop {}
}

/// `&slice[start..end]`, unchecked: `start <= end <= slice.len()`.
#[rustc_intrinsic]
pub(crate) unsafe fn subslice<T>(slice: &[T], start: usize, end: usize) -> &[T] {
    loop {}
}

/// `&mut slice[start..end]`, unchecked: `start <= end <= slice.len()`.
/// The result's lifetime is unrelated to `slice`'s, as a raw pointer's
/// would be, so that disjoint parts of one slice may be borrowed at
/// once (`split_at_mut`, `IterMut`); the caller gives it the right one.
#[rustc_intrinsic]
pub(crate) unsafe fn subslice_mut<'a, 'b, T>(
    slice: &'a mut [T],
    start: usize,
    end: usize,
) -> &'b mut [T] {
    loop {}
}

/// The `len` elements from `data` on, as a slice, unchecked: they must
/// be there for as long as the slice is used.
#[rustc_intrinsic]
pub(crate) unsafe fn slice_from_raw_parts<'a, T>(data: *const T, len: usize) -> &'a [T] {
    loop {}
}

/// Reads only a raw slice pointer's length metadata, without making a
/// reference or accessing the pointed-to elements.
#[rustc_intrinsic]
pub(crate) const fn raw_slice_len<T>(ptr: *const [T]) -> usize {
    loop {}
}

/// The `len` elements from `data` on, as a mutable slice, unchecked.
#[rustc_intrinsic]
pub(crate) unsafe fn slice_from_raw_parts_mut<'a, T>(data: *mut T, len: usize) -> &'a mut [T] {
    loop {}
}

/// The C string at `ptr`, which must end in NUL and stay there for as
/// long as the reference is used. A `&CStr` is the pointer itself.
#[rustc_intrinsic]
pub(crate) unsafe fn c_str_from_ptr<'a>(ptr: *const u8) -> &'a crate::ffi::CStr {
    loop {}
}

/// Where the `#[track_caller]` function calling this was called, or
/// where this is called in another function. Code generation passes
/// each `#[track_caller]` function its caller's location.
#[rustc_intrinsic]
pub(crate) fn caller_location() -> &'static crate::panic::Location<'static> {
    loop {}
}

/// An alignment-one scalar read from initialized, readable memory.
/// Monomorphization rejects types outside the bounded unsigned subset.
#[rustc_intrinsic]
pub(crate) unsafe fn read_unaligned<T>(src: *const T) -> T {
    loop {}
}
