//! Operations the compiler implements, which `core` builds its arrays'
//! and slices' methods from. A call of one is replaced with what it does
//! once its types are known, when generic code is monomorphized, so
//! their bodies never run; their signatures are what the borrow checker
//! sees.

/// Allocation-preserving pointer arithmetic. The caller must keep the result
/// within the allocation or at its end, with an isize-sized displacement.
#[rustc_intrinsic]
pub(crate) const unsafe fn ptr_add<T>(pointer: *const T, count: usize) -> *const T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) const unsafe fn ptr_sub<T>(pointer: *const T, count: usize) -> *const T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) const unsafe fn ptr_offset<T>(pointer: *const T, count: isize) -> *const T {
    loop {}
}

/// A bitwise copy of `*src`, which need not be `Copy`: the caller makes
/// sure that only one of the two is used and dropped.
#[rustc_intrinsic]
pub(crate) const unsafe fn read<T>(src: *const T) -> T {
    loop {}
}

/// Stores `value` in `*dst` without dropping what was there.
#[rustc_intrinsic]
pub(crate) const unsafe fn write<T>(dst: *mut T, value: T) {
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

/// The number of leading zero bits of the integer `x`: its width for 0.
#[rustc_intrinsic]
pub(crate) const fn ctlz<T>(x: T) -> u32 {
    loop {}
}

/// The number of trailing zero bits of the integer `x`: its width for 0.
#[rustc_intrinsic]
pub(crate) const fn cttz<T>(x: T) -> u32 {
    loop {}
}

/// The number of set bits of the integer `x`.
#[rustc_intrinsic]
pub(crate) const fn ctpop<T>(x: T) -> u32 {
    loop {}
}

/// The integer `x` with its bytes in reverse order.
#[rustc_intrinsic]
pub(crate) const fn bswap<T>(x: T) -> T {
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
pub(crate) const fn needs_drop<T: ?Sized>() -> bool {
    loop {}
}

#[rustc_intrinsic]
pub(crate) const fn discriminant_value<T>(value: &T) -> u128 {
    loop {}
}

#[rustc_intrinsic]
pub(crate) const fn discriminant_is_signed<T>() -> bool {
    loop {}
}

/// `&slice[start..end]`, unchecked: `start <= end <= slice.len()`.
#[rustc_intrinsic]
pub(crate) const unsafe fn subslice<T>(slice: &[T], start: usize, end: usize) -> &[T] {
    loop {}
}

/// `&mut slice[start..end]`, unchecked: `start <= end <= slice.len()`.
/// The result's lifetime is unrelated to `slice`'s, as a raw pointer's
/// would be, so that disjoint parts of one slice may be borrowed at
/// once (`split_at_mut`, `IterMut`); the caller gives it the right one.
#[rustc_intrinsic]
pub(crate) const unsafe fn subslice_mut<'a, 'b, T>(
    slice: &'a mut [T],
    start: usize,
    end: usize,
) -> &'b mut [T] {
    loop {}
}

/// The `len` elements from `data` on, as a slice, unchecked: they must
/// be there for as long as the slice is used.
#[rustc_intrinsic]
pub(crate) const unsafe fn slice_from_raw_parts<'a, T>(data: *const T, len: usize) -> &'a [T] {
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
pub(crate) const unsafe fn slice_from_raw_parts_mut<'a, T>(data: *mut T, len: usize) -> &'a mut [T] {
    loop {}
}

#[rustc_intrinsic]
pub(crate) const fn raw_slice_from_parts<T>(data: *const T, len: usize) -> *const [T] {
    loop {}
}

#[rustc_intrinsic]
pub(crate) const fn raw_slice_from_parts_mut<T>(data: *mut T, len: usize) -> *mut [T] {
    loop {}
}

/// The C string at `ptr`, which must end in NUL and stay there for as
/// long as the reference is used.
pub(crate) unsafe fn c_str_from_ptr<'a>(ptr: *const u8) -> &'a crate::ffi::CStr {
    let mut len = 0;
    // SAFETY: the caller guarantees an accessible NUL-terminated byte sequence.
    while unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: the same allocation includes the terminating byte. CStr uses
    // the slice's length metadata, including that byte, and does not mutate it.
    unsafe { &*(raw_slice_from_parts(ptr, len + 1) as *const crate::ffi::CStr) }
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

/// A static diagnostic description, emitted after type substitution.
#[rustc_intrinsic]
pub(crate) fn type_name<T: ?Sized>() -> &'static str {
    loop {}
}

// Private, naturally aligned usize-only atomic bridge. RMW and strong CAS
// return the OLD scalar. Mono rejects every other substitution.
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_load_relaxed<T>(p: *const T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_load_acquire<T>(p: *const T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_load_seqcst<T>(p: *const T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_store_relaxed<T>(p: *mut T, value: T) {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_store_release<T>(p: *mut T, value: T) {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_store_seqcst<T>(p: *mut T, value: T) {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_add_relaxed<T>(p: *mut T, value: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_add_acquire<T>(p: *mut T, value: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_add_release<T>(p: *mut T, value: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_add_acqrel<T>(p: *mut T, value: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_add_seqcst<T>(p: *mut T, value: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_sub_relaxed<T>(p: *mut T, value: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_sub_acquire<T>(p: *mut T, value: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_sub_release<T>(p: *mut T, value: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_sub_acqrel<T>(p: *mut T, value: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_sub_seqcst<T>(p: *mut T, value: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_cas_relaxed<T>(p: *mut T, expected: T, new: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_cas_acquire<T>(p: *mut T, expected: T, new: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_cas_release<T>(p: *mut T, expected: T, new: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_cas_acqrel<T>(p: *mut T, expected: T, new: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_cas_seqcst<T>(p: *mut T, expected: T, new: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_fence_acquire() {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_fence_release() {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_fence_acqrel() {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_usize_fence_seqcst() {
    loop {}
}

// Private u32-only bridge; mono validates the exact substituted word.
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u32_load_relaxed<T>(p: *const T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u32_load_acquire<T>(p: *const T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u32_load_seqcst<T>(p: *const T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u32_store_relaxed<T>(p: *mut T, value: T) {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u32_store_release<T>(p: *mut T, value: T) {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u32_store_seqcst<T>(p: *mut T, value: T) {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u32_add_relaxed<T>(p: *mut T, value: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u32_add_acquire<T>(p: *mut T, value: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u32_add_release<T>(p: *mut T, value: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u32_add_acqrel<T>(p: *mut T, value: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u32_add_seqcst<T>(p: *mut T, value: T) -> T {
    loop {}
}

// Private pointer-only bridge; T must be a thin mutable raw pointer.
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_ptr_load_relaxed<T>(p: *const T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_ptr_load_acquire<T>(p: *const T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_ptr_load_seqcst<T>(p: *const T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_ptr_store_relaxed<T>(p: *mut T, value: T) {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_ptr_store_release<T>(p: *mut T, value: T) {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_ptr_store_seqcst<T>(p: *mut T, value: T) {
    loop {}
}

// Private byte atomic bridge; storage is naturally aligned at one byte.
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u8_load_relaxed<T>(p: *const T) -> T { loop {} }
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u8_load_acquire<T>(p: *const T) -> T { loop {} }
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u8_load_seqcst<T>(p: *const T) -> T { loop {} }
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u8_store_relaxed<T>(p: *mut T, value: T) { loop {} }
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u8_store_release<T>(p: *mut T, value: T) { loop {} }
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u8_store_seqcst<T>(p: *mut T, value: T) { loop {} }

// Private halfword atomic bridge; storage is naturally aligned at one halfword.
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u16_load_relaxed<T>(p: *const T) -> T { loop {} }
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u16_load_acquire<T>(p: *const T) -> T { loop {} }
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u16_load_seqcst<T>(p: *const T) -> T { loop {} }
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u16_store_relaxed<T>(p: *mut T, value: T) { loop {} }
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u16_store_release<T>(p: *mut T, value: T) { loop {} }
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u16_store_seqcst<T>(p: *mut T, value: T) { loop {} }

// Volatile memory operations retain their externally observable access flags.
#[rustc_intrinsic]
pub(crate) unsafe fn read_volatile<T>(src:*const T)->T{loop{}}
#[rustc_intrinsic]
pub(crate) unsafe fn write_volatile<T>(dst:*mut T,value:T){loop{}}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u8_cas_relaxed<T>(p: *mut T, expected: T, new: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u8_cas_acquire<T>(p: *mut T, expected: T, new: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u8_cas_release<T>(p: *mut T, expected: T, new: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u8_cas_acqrel<T>(p: *mut T, expected: T, new: T) -> T {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u8_cas_seqcst<T>(p: *mut T, expected: T, new: T) -> T {
    loop {}
}

/// Opaque identity of a fully substituted static type.
#[rustc_intrinsic]
pub(crate) const fn type_id<T: ?Sized + 'static>() -> u128 {
    loop {}
}

#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u16_cas_relaxed<T>(p:*mut T,expected:T,new:T)->T {loop {}}
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u16_cas_acquire<T>(p:*mut T,expected:T,new:T)->T {loop {}}
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u16_cas_release<T>(p:*mut T,expected:T,new:T)->T {loop {}}
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u16_cas_acqrel<T>(p:*mut T,expected:T,new:T)->T {loop {}}
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u16_cas_seqcst<T>(p:*mut T,expected:T,new:T)->T {loop {}}
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u32_cas_relaxed<T>(p:*mut T,expected:T,new:T)->T {loop {}}
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u32_cas_acquire<T>(p:*mut T,expected:T,new:T)->T {loop {}}
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u32_cas_release<T>(p:*mut T,expected:T,new:T)->T {loop {}}
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u32_cas_acqrel<T>(p:*mut T,expected:T,new:T)->T {loop {}}
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_u32_cas_seqcst<T>(p:*mut T,expected:T,new:T)->T {loop {}}
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_ptr_cas_relaxed<T>(p:*mut T,expected:T,new:T)->T {loop {}}
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_ptr_cas_acquire<T>(p:*mut T,expected:T,new:T)->T {loop {}}
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_ptr_cas_release<T>(p:*mut T,expected:T,new:T)->T {loop {}}
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_ptr_cas_acqrel<T>(p:*mut T,expected:T,new:T)->T {loop {}}
#[rustc_intrinsic]
pub(crate) unsafe fn __atomic_ptr_cas_seqcst<T>(p:*mut T,expected:T,new:T)->T {loop {}}

/// Changes the data address without changing pointer metadata.
#[rustc_intrinsic]
pub(crate) fn ptr_with_addr<T:?Sized>(pointer:*const T,address:usize)->*const T {loop {}}

/// The pointer metadata word, or zero for a thin pointer. This private
/// operation preserves the compiler's pointer representation without reads
/// from the pointee or assumptions about the in-memory layout of a fat pointer.
#[rustc_intrinsic]
pub(crate) fn ptr_metadata_word<T: ?Sized>(pointer: *const T) -> usize { loop {} }
