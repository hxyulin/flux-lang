//! Slices' methods, and their iterators.

use crate::clone::Clone;
use crate::cmp::Ordering::{self, Equal, Greater, Less};
use crate::cmp::{Eq, Ord, PartialEq, PartialOrd};
use crate::intrinsics::{read, subslice, subslice_mut, write};
use crate::iter::{DoubleEndedIterator, ExactSizeIterator, IntoIterator, Iterator};
use crate::marker::Copy;
use crate::ops::{
    Bound, FnMut, Index, IndexMut, Range, RangeBounds, RangeFrom, RangeFull, RangeInclusive,
    RangeTo, RangeToInclusive,
};
use crate::option::Option::{self, None, Some};
use crate::panicking::panic;
use crate::result::Result::{self, Err, Ok};

mod sort;

/// Views one initialized value as a shared one-element slice.
pub const fn from_ref<T>(value: &T) -> &[T] {
    // SAFETY: [T; 1] has the same layout as T. The array reference borrows
    // the same initialized value for the input lifetime, then unsizes.
    unsafe { &*(value as *const T as *const [T; 1]) }
}

/// Views one uniquely borrowed value as a mutable one-element slice.
pub const fn from_mut<T>(value: &mut T) -> &mut [T] {
    // SAFETY: [T; 1] has T's layout; the unique input borrow is retained by
    // the result. No second mutable reference or additional element exists.
    unsafe { &mut *(value as *mut T as *mut [T; 1]) }
}

/// The `len` values from `data` on, as a slice, which must be valid for
/// as long as the slice is used.
#[inline(always)]
pub const unsafe fn from_raw_parts<'a, T>(data: *const T, len: usize) -> &'a [T] {
    unsafe { crate::intrinsics::slice_from_raw_parts(data, len) }
}

/// The `len` values from `data` on, as a mutable slice.
#[inline(always)]
pub const unsafe fn from_raw_parts_mut<'a, T>(data: *mut T, len: usize) -> &'a mut [T] {
    unsafe { crate::intrinsics::slice_from_raw_parts_mut(data, len) }
}

impl<T> [T] {
    /// Splits this slice into a prefix, an aligned middle view, and a suffix.
    ///
    /// The middle covers as many whole `T` and `U` elements as possible after
    /// reaching `U`'s alignment. If either type is zero-sized, the entire input
    /// remains in the prefix and both other slices are empty.
    ///
    /// # Safety
    /// The bytes in the middle must be valid initialized `U` values. The
    /// caller must also preserve the aliasing requirements of any references
    /// contained in those values, as for a shared transmute of the elements.
    pub unsafe fn align_to<U>(&self) -> (&[T], &[U], &[T]) {
        let t_size = crate::mem::size_of::<T>();
        let u_size = crate::mem::size_of::<U>();
        if t_size == 0 || u_size == 0 {
            return (self, &[], &[]);
        }
        let offset = self.as_ptr().align_offset(crate::mem::align_of::<U>());
        if offset > self.len() {
            return (self, &[], &[]);
        }
        let (prefix, rest) = self.split_at(offset);
        let mut a = t_size;
        let mut b = u_size;
        while b != 0 {
            let remainder = a % b;
            a = b;
            b = remainder;
        }
        // Whole blocks contain u_size/gcd T elements and t_size/gcd U
        // elements. Counting blocks first avoids overflowing a byte count.
        let t_block = u_size / a;
        let u_block = t_size / a;
        let middle_len = rest.len() / t_block * u_block;
        let suffix_len = rest.len() % t_block;
        // SAFETY: align_offset established the middle alignment. Whole blocks
        // stay inside the original allocation, and the suffix starts at a T
        // element boundary. The caller establishes the middle's U validity;
        // all three shared slices borrow the original allocation and lifetime.
        unsafe {
            (
                prefix,
                from_raw_parts(rest.as_ptr() as *const U, middle_len),
                from_raw_parts(rest.as_ptr().add(rest.len() - suffix_len), suffix_len),
            )
        }
    }

    /// Splits mutably into an unaligned prefix, a middle of `U` values and a
    /// suffix, as align_to does.
    ///
    /// # Safety
    /// As for align_to: the middle's bytes must be valid `U` values, and
    /// writes through either view must keep the other's values valid.
    pub unsafe fn align_to_mut<U>(&mut self) -> (&mut [T], &mut [U], &mut [T]) {
        let len = self.len();
        let base = self.as_mut_ptr();
        // SAFETY: the three shared pieces partition this exclusive borrow, so
        // rebuilding them as mutable slices creates no overlapping access.
        unsafe {
            let (prefix, middle, suffix) = self.align_to::<U>();
            let (prefix_len, middle_len, suffix_len) = (prefix.len(), middle.len(), suffix.len());
            let middle_ptr = middle.as_ptr() as *mut U;
            (
                crate::slice::from_raw_parts_mut(base, prefix_len),
                crate::slice::from_raw_parts_mut(middle_ptr, middle_len),
                crate::slice::from_raw_parts_mut(base.add(len - suffix_len), suffix_len),
            )
        }
    }

    /// The number of elements in the slice.
    pub const fn len(&self) -> usize {
        crate::intrinsics::raw_slice_len(self as *const [T])
    }

    /// Splits into arrays of `N` elements and a remaining suffix.
    /// Panics if `N` is zero.
    #[track_caller]
    pub fn as_chunks<const N: usize>(&self) -> (&[[T; N]], &[T]) {
        assert!(N != 0, "chunk size must be non-zero");
        let chunks = self.len() / N;
        let (whole, remainder) = self.split_at(chunks * N);
        // SAFETY: the prefix contains exactly chunks contiguous arrays, with
        // T's alignment. Array grouping neither copies nor changes elements.
        let whole = unsafe { from_raw_parts(whole.as_ptr() as *const [T; N], chunks) };
        (whole, remainder)
    }

    /// Splits into a remaining prefix and arrays of `N` elements.
    /// Panics if `N` is zero.
    #[track_caller]
    pub fn as_rchunks<const N: usize>(&self) -> (&[T], &[[T; N]]) {
        assert!(N != 0, "chunk size must be non-zero");
        let (remainder, whole) = self.split_at(self.len() % N);
        // SAFETY: the suffix contains exactly whole.len()/N contiguous
        // arrays. Grouping preserves T's alignment and the original borrow.
        let whole = unsafe {
            from_raw_parts(whole.as_ptr() as *const [T; N], whole.len() / N)
        };
        (remainder, whole)
    }

    /// Splits into disjoint mutable arrays of `N` elements and a suffix.
    /// Panics if `N` is zero.
    #[track_caller]
    pub fn as_chunks_mut<const N: usize>(&mut self) -> (&mut [[T; N]], &mut [T]) {
        assert!(N != 0, "chunk size must be non-zero");
        let chunks = self.len() / N;
        let (whole, remainder) = self.split_at_mut(chunks * N);
        // SAFETY: split_at_mut made these ranges disjoint; grouping the exact
        // prefix into arrays preserves alignment and unique ownership.
        let whole = unsafe { from_raw_parts_mut(whole.as_mut_ptr() as *mut [T; N], chunks) };
        (whole, remainder)
    }

    /// The first element, or `None` if the slice is empty.
    pub const fn first(&self) -> Option<&T> {
        if self.len() == 0 { None } else { Some(&self[0]) }
    }

    /// The last element, or `None` if the slice is empty.
    pub const fn last(&self) -> Option<&T> {
        let len = self.len();
        if len == 0 { None } else { Some(&self[len - 1]) }
    }

    /// The element at `index`, or the part a range of indices gives, or
    /// `None` if it is out of bounds.
    pub fn get<I: SliceIndex<[T]>>(&self, index: I) -> Option<&I::Output> {
        index.get(self)
    }

    /// Returns an element or subslice without checking the index's validity.
    ///
    /// # Safety
    /// The index must be valid for this slice, even if the result is unused.
    pub unsafe fn get_unchecked<I: SliceIndex<[T]>>(&self, index: I) -> &I::Output {
        // SAFETY: the caller guarantees the index is in bounds.
        unsafe { index.get(self).unwrap_unchecked() }
    }

    /// Returns an element or subslice mutably without checking validity.
    ///
    /// # Safety
    /// The index must be valid for this slice, even if the result is unused.
    pub unsafe fn get_unchecked_mut<I: SliceIndex<[T]>>(&mut self, index: I) -> &mut I::Output {
        // SAFETY: the caller guarantees the index is in bounds.
        unsafe { index.get_mut(self).unwrap_unchecked() }
    }

    /// An iterator over references to the elements, in order.
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            slice: self,
            start: 0,
            end: self.len(),
        }
    }

    /// An iterator over mutable references to the elements, in order.
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        let end = self.len();
        IterMut {
            ptr: self.as_mut_ptr(),
            start: 0,
            end,
            marker: crate::marker::PhantomData,
        }
    }

    /// The first element, or `None` if the slice is empty, mutably.
    pub fn first_mut(&mut self) -> Option<&mut T> {
        if self.len() == 0 { None } else { Some(&mut self[0]) }
    }

    /// The last element, or `None` if the slice is empty, mutably.
    pub fn last_mut(&mut self) -> Option<&mut T> {
        let len = self.len();
        if len == 0 { None } else { Some(&mut self[len - 1]) }
    }

    /// The element at `index`, or the part a range of indices gives, or
    /// `None` if it is out of bounds, mutably.
    pub fn get_mut<I: SliceIndex<[T]>>(&mut self, index: I) -> Option<&mut I::Output> {
        index.get_mut(self)
    }

    /// The first element and the rest, or `None` if the slice is empty.
    pub const fn split_first(&self) -> Option<(&T, &[T])> {
        if self.len() == 0 { None } else {
            let (first, rest) = self.split_at(1);
            Some((&first[0], rest))
        }
    }

    /// The first N elements, or None when the slice is shorter.
    pub const fn first_chunk<const N:usize>(&self)->Option<&[T;N]> {
        if self.len()<N {None} else {
            // SAFETY: the checked prefix has N initialized elements.
            unsafe {Some(&*(self.as_ptr() as *const [T;N]))}
        }
    }
    /// The last N elements, or None when the slice is shorter.
    pub const fn last_chunk<const N:usize>(&self)->Option<&[T;N]> {
        if self.len()<N {None} else {
            // SAFETY: the checked suffix has N initialized elements, including
            // the valid one-past pointer used by an empty suffix.
            unsafe {Some(&*(self.as_ptr().add(self.len()-N) as *const [T;N]))}
        }
    }
    /// Exclusive access to the first N elements.
    pub const fn first_chunk_mut<const N:usize>(&mut self)->Option<&mut [T;N]> {
        if self.len()<N {None} else {
            // SAFETY: the prefix fits and the slice is exclusively borrowed.
            unsafe {Some(&mut *(self.as_mut_ptr() as *mut [T;N]))}
        }
    }
    /// Exclusive access to the last N elements.
    pub const fn last_chunk_mut<const N:usize>(&mut self)->Option<&mut [T;N]> {
        if self.len()<N {None} else {
            // SAFETY: the suffix fits and the slice is exclusively borrowed.
            unsafe {Some(&mut *(self.as_mut_ptr().add(self.len()-N) as *mut [T;N]))}
        }
    }

    /// The first `N` elements as an array and the remaining slice, or
    /// `None` if fewer than `N` elements remain.
    pub fn split_first_chunk<const N: usize>(&self) -> Option<(&[T; N], &[T])> {
        if self.len() < N {
            None
        } else {
            // SAFETY: the length check ensures the array fits. Its alignment
            // is T's alignment, and both references borrow the original slice.
            // For N = 0 the prefix is empty and the suffix is the whole slice.
            let first = unsafe { &*(self.as_ptr() as *const [T; N]) };
            Some((first, &self[N..]))
        }
    }

    /// The remaining prefix and the last `N` elements as an array.
    /// Returns `None` when the slice is shorter than `N`.
    pub fn split_last_chunk<const N: usize>(&self) -> Option<(&[T], &[T; N])> {
        let last = self.last_chunk::<N>()?;
        Some((&self[..self.len() - N], last))
    }

    /// The last element and the rest, or `None` if the slice is empty.
    pub fn split_last(&self) -> Option<(&T, &[T])> {
        let len = self.len();
        if len == 0 { None } else { Some((&self[len - 1], &self[..len - 1])) }
    }

    /// Swaps the elements at `a` and `b`; panics if either is out of
    /// bounds.
    pub fn swap(&mut self, a: usize, b: usize) {
        let x = unsafe { read(&self[a]) };
        let y = unsafe { read(&self[b]) };
        unsafe {
            write(&mut self[a], y);
            write(&mut self[b], x);
        }
    }

    /// Reverses the order of the elements.
    pub fn reverse(&mut self) {
        let len = self.len();
        let mut i = 0;
        while i < len / 2 {
            self.swap(i, len - 1 - i);
            i += 1;
        }
    }

    /// Rotates the slice so that the element at `mid` comes first;
    /// panics if `mid > len`.
    #[track_caller]
    pub fn rotate_left(&mut self, mid: usize) {
        if mid > self.len() {
            panic("assertion failed: mid <= self.len()")
        }
        sort::rotate(self, mid);
    }

    /// Rotates the slice so that the last `k` elements come first;
    /// panics if `k > len`.
    #[track_caller]
    pub fn rotate_right(&mut self, k: usize) {
        let len = self.len();
        if k > len {
            panic("assertion failed: k <= self.len()")
        }
        sort::rotate(self, len - k);
    }

    /// The slice split at `mid`: `[0, mid)` and `[mid, len)`; panics if
    /// `mid > len`.
    #[track_caller]
    pub const fn split_at(&self, mid: usize) -> (&[T], &[T]) {
        let len = self.len();
        if mid > len {
            panic("mid > len")
        }
        unsafe { (subslice(self, 0, mid), subslice(self, mid, len)) }
    }

    /// The slice split at `mid`, mutably: `[0, mid)` and `[mid, len)`;
    /// panics if `mid > len`.
    #[track_caller]
    pub const fn split_at_mut(&mut self, mid: usize) -> (&mut [T], &mut [T]) {
        let len = self.len();
        if mid > len {
            panic("mid > len")
        }
        unsafe { (subslice_mut(self, 0, mid), subslice_mut(self, mid, len)) }
    }

    /// Splits a slice without requiring an explicit bounds check by the caller.
    ///
    /// # Safety
    /// `mid` must not exceed the slice length.
    pub const unsafe fn split_at_unchecked(&self, mid: usize) -> (&[T], &[T]) {
        self.split_at(mid)
    }

    /// Splits a mutable slice into disjoint slices.
    ///
    /// # Safety
    /// `mid` must not exceed the slice length.
    pub const unsafe fn split_at_mut_unchecked(&mut self, mid: usize) -> (&mut [T], &mut [T]) {
        self.split_at_mut(mid)
    }

    /// Splits at `mid`, or returns `None` when it exceeds the length.
    pub const fn split_at_checked(&self, mid: usize) -> Option<(&[T], &[T])> {
        if mid > self.len() { None } else { Some(self.split_at(mid)) }
    }

    /// Splits mutably at `mid`, or returns `None` when it exceeds the length.
    pub const fn split_at_mut_checked(&mut self, mid: usize) -> Option<(&mut [T], &mut [T])> {
        if mid > self.len() { None } else { Some(self.split_at_mut(mid)) }
    }

    /// Removes a matching prefix without copying any elements.
    pub fn strip_prefix(&self, prefix: &[T]) -> Option<&[T]> where T: PartialEq {
        if prefix.len() > self.len() { return None; }
        let mut i = 0;
        while i < prefix.len() {
            if self[i] != prefix[i] { return None; }
            i += 1;
        }
        Some(&self[prefix.len()..])
    }

    /// Splits around each element accepted by `pred`, keeping empty pieces.
    pub fn split<P: FnMut(&T) -> bool>(&self, pred: P) -> Split<'_, T, P> {
        Split { slice: self, pred, finished: false }
    }

    /// Removes a matching suffix without copying any elements.
    pub fn strip_suffix(&self, suffix: &[T]) -> Option<&[T]> where T: PartialEq {
        if suffix.len() > self.len() { return None; }
        let start = self.len() - suffix.len();
        let mut i = 0;
        while i < suffix.len() {
            if self[start + i] != suffix[i] { return None; }
            i += 1;
        }
        Some(&self[..start])
    }
    /// Splits after each element accepted by `pred`, which ends its piece.
    pub fn split_inclusive<P: FnMut(&T) -> bool>(&self, pred: P) -> SplitInclusive<'_, T, P> {
        let finished = self.is_empty();
        SplitInclusive { slice: self, pred, finished }
    }
    /// Splits mutably after each element accepted by `pred`.
    pub fn split_inclusive_mut<P: FnMut(&T) -> bool>(&mut self, pred: P) -> SplitInclusiveMut<'_, T, P> {
        let finished = self.is_empty();
        SplitInclusiveMut { slice: self, pred, finished }
    }
    /// Splits around accepted elements, into at most `n` pieces.
    pub fn splitn<P: FnMut(&T) -> bool>(&self, n: usize, pred: P) -> SplitN<'_, T, P> {
        SplitN { inner: self.split(pred), count: n }
    }
    /// Splits around accepted elements, from the back.
    pub fn rsplit<P: FnMut(&T) -> bool>(&self, pred: P) -> RSplit<'_, T, P> {
        RSplit { inner: self.split(pred) }
    }
    /// Splits around accepted elements from the back, into at most `n` pieces.
    pub fn rsplitn<P: FnMut(&T) -> bool>(&self, n: usize, pred: P) -> RSplitN<'_, T, P> {
        RSplitN { inner: self.split(pred), count: n }
    }
    /// Splits mutably around each element accepted by `pred`.
    pub fn split_mut<P: FnMut(&T) -> bool>(&mut self, pred: P) -> SplitMut<'_, T, P> {
        SplitMut { slice: self, pred, finished: false }
    }
    /// Splits into runs of adjacent elements that `pred` accepts pairwise.
    pub fn chunk_by<F: FnMut(&T, &T) -> bool>(&self, pred: F) -> ChunkBy<'_, T, F> {
        ChunkBy { slice: self, predicate: pred }
    }
    /// Splits mutably into runs accepted pairwise by `pred`.
    pub fn chunk_by_mut<P: FnMut(&T, &T) -> bool>(&mut self, pred: P) -> ChunkByMut<'_, T, P> {
        ChunkByMut { slice: self, predicate: pred }
    }
    /// The first element and the rest, mutably.
    pub fn split_first_mut(&mut self) -> Option<(&mut T, &mut [T])> {
        if self.is_empty() { return None; }
        let (first, rest) = self.split_at_mut(1);
        Some((&mut first[0], rest))
    }
    /// The last element and the rest, mutably.
    pub fn split_last_mut(&mut self) -> Option<(&mut T, &mut [T])> {
        let len = self.len();
        if len == 0 { return None; }
        let (rest, last) = self.split_at_mut(len - 1);
        Some((&mut last[0], rest))
    }
    /// Swaps every element with the one at the same index of `other`.
    ///
    /// # Panics
    /// Panics if the slices differ in length.
    #[track_caller]
    pub fn swap_with_slice(&mut self, other: &mut [T]) {
        if self.len() != other.len() { panic("destination and source slices have different lengths"); }
        let mut i = 0;
        while i < self.len() {
            crate::mem::swap(&mut self[i], &mut other[i]);
            i += 1;
        }
    }
    /// Fills the slice with values returned by `f`, in order.
    pub fn fill_with<F: FnMut() -> T>(&mut self, mut f: F) {
        for slot in self.iter_mut() { *slot = f(); }
    }

    /// Copies the elements of `src`, which must be as long (or this
    /// panics), into the slice.
    #[track_caller]
    pub fn copy_from_slice(&mut self, src: &[T])
    where
        T: Copy,
    {
        if self.len() != src.len() {
            panic("copy_from_slice: source slice length does not match destination slice length")
        }
        // SAFETY: both slices hold `len` elements, a shared and a mutable
        // borrow cannot overlap, and `T` is `Copy`.
        unsafe { crate::ptr::copy_nonoverlapping(src.as_ptr(), self.as_mut_ptr(), src.len()) }
    }

    /// Clones the elements of `src`, which must be as long (or this
    /// panics), into the slice.
    #[track_caller]
    pub fn clone_from_slice(&mut self, src: &[T])
    where
        T: Clone,
    {
        if self.len() != src.len() {
            panic("destination and source slices have different lengths")
        }
        if crate::intrinsics::is_copy::<T>() {
            // SAFETY: as in `copy_from_slice`; a `Copy` element has no
            // drop to run on the value it overwrites.
            unsafe { crate::ptr::copy_nonoverlapping(src.as_ptr(), self.as_mut_ptr(), src.len()) }
            return;
        }
        let mut i = 0;
        while i < src.len() {
            self[i] = src[i].clone();
            i += 1;
        }
    }

    /// Sets every element to `value`: clones of it, and `value` itself
    /// last.
    pub fn fill(&mut self, value: T)
    where
        T: Clone,
    {
        let len = self.len();
        if len == 0 {
            return;
        }
        let mut i = 0;
        while i + 1 < len {
            self[i] = value.clone();
            i += 1;
        }
        self[len - 1] = value;
    }

    /// Whether an element equals `x`.
    pub fn contains(&self, x: &T) -> bool
    where
        T: PartialEq,
    {
        for y in self.iter() {
            if *y == *x {
                return true;
            }
        }
        false
    }

    /// Whether the slice begins with `needle`.
    pub fn starts_with(&self, needle: &[T]) -> bool
    where
        T: PartialEq,
    {
        let n = needle.len();
        self.len() >= n && needle == &self[..n]
    }

    /// Whether the slice ends with `needle`.
    pub fn ends_with(&self, needle: &[T]) -> bool
    where
        T: PartialEq,
    {
        let (m, n) = (self.len(), needle.len());
        m >= n && needle == &self[m - n..]
    }

    /// An iterator over the overlapping windows of `size` elements, in
    /// order; panics if `size` is 0.
    #[track_caller]
    pub fn windows(&self, size: usize) -> Windows<'_, T> {
        if size == 0 {
            panic("window size must be non-zero")
        }
        Windows { v: self, size }
    }

    /// Overlapping windows of `N` elements. Panics if `N` is zero.
    #[track_caller]
    pub fn array_windows<const N: usize>(&self) -> ArrayWindows<'_, T, N> {
        ArrayWindows { inner: self.windows(N) }
    }

    /// An iterator over chunks of `chunk_size` elements, in order, the
    /// last one shorter if the length is not a multiple; panics if
    /// `chunk_size` is 0.
    #[track_caller]
    pub fn chunks(&self, chunk_size: usize) -> Chunks<'_, T> {
        if chunk_size == 0 {
            panic("chunk size must be non-zero")
        }
        Chunks {
            v: self,
            chunk_size,
        }
    }

    /// An iterator over chunks of exactly `chunk_size` elements, in order,
    /// without the remainder; panics if `chunk_size` is 0.
    #[track_caller]
    pub fn chunks_exact(&self, chunk_size: usize) -> ChunksExact<'_, T> {
        if chunk_size == 0 {
            panic("chunk size must be non-zero")
        }
        let len = self.len();
        let fst_len = len - len % chunk_size;
        let (fst, snd) = self.split_at(fst_len);
        ChunksExact {
            v: fst,
            rem: snd,
            chunk_size,
        }
    }

    /// An iterator over mutable chunks of `chunk_size` elements, in
    /// order, the last one shorter if the length is not a multiple;
    /// panics if `chunk_size` is 0.
    #[track_caller]
    pub fn chunks_mut(&mut self, chunk_size: usize) -> ChunksMut<'_, T> {
        if chunk_size == 0 {
            panic("chunk size must be non-zero")
        }
        let len = self.len();
        ChunksMut {
            v: self,
            start: 0,
            end: len,
            chunk_size,
        }
    }

    /// An iterator over mutable chunks of exactly `chunk_size` elements,
    /// in order, without the remainder; panics if `chunk_size` is 0.
    #[track_caller]
    pub fn chunks_exact_mut(&mut self, chunk_size: usize) -> ChunksExactMut<'_, T> {
        if chunk_size == 0 {
            panic("chunk size must be non-zero")
        }
        let len = self.len();
        let fst_len = len - len % chunk_size;
        let (fst, snd) = self.split_at_mut(fst_len);
        ChunksExactMut {
            v: fst,
            start: 0,
            end: fst_len,
            rem: snd,
            chunk_size,
        }
    }

    /// An iterator over chunks of `chunk_size` elements from the end, the
    /// last one (the slice's start) shorter if the length is not a
    /// multiple; panics if `chunk_size` is 0.
    #[track_caller]
    pub fn rchunks(&self, chunk_size: usize) -> RChunks<'_, T> {
        if chunk_size == 0 {
            panic("chunk size must be non-zero")
        }
        RChunks {
            v: self,
            chunk_size,
        }
    }

    /// An iterator over chunks of exactly `chunk_size` elements from the
    /// end, without the remainder at the start; panics if `chunk_size` is
    /// 0.
    #[track_caller]
    pub fn rchunks_exact(&self, chunk_size: usize) -> RChunksExact<'_, T> {
        if chunk_size == 0 {
            panic("chunk size must be non-zero")
        }
        let rem = self.len() % chunk_size;
        let (fst, snd) = self.split_at(rem);
        RChunksExact {
            v: snd,
            rem: fst,
            chunk_size,
        }
    }

    /// Mutable chunks from the end. Panics if `chunk_size` is zero.
    #[track_caller]
    pub fn rchunks_mut(&mut self, chunk_size: usize) -> RChunksMut<'_, T> {
        if chunk_size == 0 { panic("chunk size must be non-zero") }
        RChunksMut { slice: self, chunk_size }
    }
    /// Exact mutable chunks from the end, excluding the initial remainder.
    #[track_caller]
    pub fn rchunks_exact_mut(&mut self, chunk_size: usize) -> RChunksExactMut<'_, T> {
        if chunk_size == 0 { panic("chunk size must be non-zero") }
        let remainder = self.len() % chunk_size;
        let (rem, slice) = self.split_at_mut(remainder);
        RChunksExactMut { inner: slice.chunks_exact_mut(chunk_size), rem }
    }

    /// Copies the elements of `src`, a range of indices, to the ones from
    /// `dest` on, which may overlap them; panics if either is out of
    /// bounds.
    #[track_caller]
    pub fn copy_within<R: RangeBounds<usize>>(&mut self, src: R, dest: usize)
    where
        T: Copy,
    {
        let len = self.len();
        let r = bounds_range(src, len);
        let count = r.end - r.start;
        if dest > len - count {
            panic("dest is out of bounds")
        }
        if dest <= r.start {
            let mut i = 0;
            while i < count {
                self[dest + i] = self[r.start + i];
                i += 1;
            }
        } else {
            let mut i = count;
            while i > 0 {
                i -= 1;
                self[dest + i] = self[r.start + i];
            }
        }
    }

    /// Where `x` is in the slice, which must be sorted: `Ok` of the index
    /// of an equal element, or `Err` of where it would be inserted.
    pub fn binary_search(&self, x: &T) -> Result<usize, usize>
    where
        T: Ord,
    {
        self.binary_search_by(|p| p.cmp(x))
    }

    /// [`Self::binary_search`] with `f` saying how each element compares
    /// with what is looked for. As rustc's, it halves the range without
    /// stopping at an equal element, so it finds the same one.
    pub fn binary_search_by<'a, F: FnMut(&'a T) -> Ordering>(&'a self, mut f: F) -> Result<usize, usize> {
        binary_search(self, &mut f)
    }

    /// [`Self::binary_search`] on the keys `f` gives.
    pub fn binary_search_by_key<'a, B: Ord, F: FnMut(&'a T) -> B>(
        &'a self,
        b: &B,
        mut f: F,
    ) -> Result<usize, usize> {
        binary_search(self, &mut |k: &T| f(k).cmp(b))
    }

    /// The index separating the initial elements for which `pred` is true
    /// from the remaining elements for which it is false.
    ///
    /// The slice must be partitioned by `pred`; otherwise the result is
    /// unspecified and meaningless. Returns 0 for an empty slice or when
    /// no elements match, and `self.len()` when all elements match.
    ///
    /// Uses binary search with logarithmically many predicate calls. The
    /// predicate may mutate captured state, but its answers must preserve
    /// the partition. Call order and the exact number of calls are not
    /// guaranteed.
    pub fn partition_point<P: FnMut(&T) -> bool>(&self, mut pred: P) -> usize {
        let mut left = 0;
        let mut right = self.len();
        while left < right {
            let mid = left + (right - left) / 2;
            if pred(&self[mid]) {
                left = mid + 1;
            } else {
                right = mid;
            }
        }
        left
    }

    /// Whether the elements are in order.
    pub fn is_sorted(&self) -> bool
    where
        T: PartialOrd,
    {
        self.is_sorted_by(|a: &T, b: &T| a <= b)
    }

    /// Whether every adjacent pair is accepted by `compare`.
    pub fn is_sorted_by<'a, F: FnMut(&'a T, &'a T) -> bool>(&'a self, mut compare: F) -> bool {
        let mut i = 1;
        while i < self.len() {
            if !compare(&self[i - 1], &self[i]) { return false; }
            i += 1;
        }
        true
    }

    /// Whether the keys are in nondecreasing order.
    pub fn is_sorted_by_key<'a, K: PartialOrd, F: FnMut(&'a T) -> K>(&'a self, mut f: F) -> bool {
        let mut iter = self.iter();
        let mut previous = match iter.next() { Some(value) => f(value), None => return true };
        for value in iter {
            let next = f(value);
            if !(previous <= next) { return false; }
            previous = next;
        }
        true
    }

    /// Partitions around the element at `index` in sorted order.
    ///
    /// # Panics
    /// Panics when `index` is out of bounds.
    pub fn select_nth_unstable(&mut self, index: usize) -> (&mut [T], &mut T, &mut [T])
    where T: Ord {
        self.select_nth_unstable_by(index, |a: &T, b: &T| a.cmp(b))
    }
    /// Partitions in linear time using `compare`, without allocating.
    ///
    /// # Panics
    /// Panics when `index` is out of bounds or `compare` panics.
    pub fn select_nth_unstable_by<F: FnMut(&T, &T) -> Ordering>(
        &mut self, index: usize, mut compare: F,
    ) -> (&mut [T], &mut T, &mut [T]) {
        assert!(index < self.len(), "selection index out of bounds");
        sort::select(self, index, &mut |a: &T, b: &T| compare(a, b) == Less);
        let (before, rest) = self.split_at_mut(index);
        let (pivot, after) = rest.split_first_mut().unwrap();
        (before, pivot, after)
    }
    /// Partitions by the keys produced by `f`, without allocating.
    ///
    /// # Panics
    /// Panics when `index` is out of bounds or `f` panics.
    pub fn select_nth_unstable_by_key<K: Ord, F: FnMut(&T) -> K>(
        &mut self, index: usize, mut f: F,
    ) -> (&mut [T], &mut T, &mut [T]) {
        self.select_nth_unstable_by(index, |a: &T, b: &T| f(a).cmp(&f(b)))
    }

    /// Sorts the slice, keeping equal elements in order.
    pub fn sort(&mut self)
    where
        T: Ord,
    {
        sort::stable(self, &mut |a: &T, b: &T| a.lt(b));
    }

    /// Sorts the slice by `compare`, keeping equal elements in order.
    pub fn sort_by<F: FnMut(&T, &T) -> Ordering>(&mut self, mut compare: F) {
        sort::stable(self, &mut |a: &T, b: &T| compare(a, b) == Less);
    }

    /// Sorts the slice by the keys `f` gives, keeping equal elements in
    /// order.
    pub fn sort_by_key<K: Ord, F: FnMut(&T) -> K>(&mut self, mut f: F) {
        sort::stable(self, &mut |a: &T, b: &T| f(a).lt(&f(b)));
    }

    /// Sorts the slice; equal elements may be reordered (flux's keeps
    /// them in order, as rustc's does for 20 elements or fewer).
    pub fn sort_unstable(&mut self)
    where
        T: Ord,
    {
        sort::stable(self, &mut |a: &T, b: &T| a.lt(b));
    }

    /// Sorts the slice by `compare`; equal elements may be reordered.
    pub fn sort_unstable_by<F: FnMut(&T, &T) -> Ordering>(&mut self, mut compare: F) {
        sort::stable(self, &mut |a: &T, b: &T| compare(a, b) == Less);
    }

    /// Sorts the slice by the keys `f` gives; equal elements may be
    /// reordered.
    pub fn sort_unstable_by_key<K: Ord, F: FnMut(&T) -> K>(&mut self, mut f: F) {
        sort::stable(self, &mut |a: &T, b: &T| f(a).lt(&f(b)));
    }
}

/// An iterator over a slice's elements by reference: what
/// [`[T]::iter`] and `for x in slice` give.
#[lang = "slice_iter"]
pub struct Iter<'a, T> {
    slice: &'a [T],
    start: usize,
    end: usize,
}

impl<'a, T> Iter<'a, T> {
    /// The iterator's remaining elements, with the original slice's lifetime.
    pub fn as_slice(&self) -> &'a [T] {
        &self.slice[self.start..self.end]
    }
}

impl<'a, T> crate::clone::Clone for Iter<'a, T> {
    fn clone(&self) -> Self {
        Iter {
            slice: self.slice,
            start: self.start,
            end: self.end,
        }
    }
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    const __TRUSTED_LEN: bool = true;
    const __RANDOM_ACCESS: bool = true;
    const __MAY_HAVE_SIDE_EFFECT: bool = false;

    fn __iterator_get_unchecked(&mut self, idx: usize) -> &'a T {
        &self.slice[self.start + idx]
    }

    fn next(&mut self) -> Option<&'a T> {
        if self.start < self.end {
            let item = &self.slice[self.start];
            self.start += 1;
            Some(item)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.end - self.start;
        (n, Some(n))
    }
}

impl<'a, T> ExactSizeIterator for Iter<'a, T> {}
impl<'a, T> crate::iter::FusedIterator for Iter<'a, T> {}

impl<'a, T> DoubleEndedIterator for Iter<'a, T> {
    fn next_back(&mut self) -> Option<&'a T> {
        if self.start < self.end {
            self.end -= 1;
            Some(&self.slice[self.end])
        } else {
            None
        }
    }
}

impl<'a, T> IntoIterator for &'a [T] {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}

impl<T> [T] {
    /// A pointer to the first element, for C: valid while the slice is.
    pub const fn as_ptr(&self) -> *const T {
        self as *const [T] as *const T
    }

    /// The half-open pointer range of the slice, including its end pointer.
    pub fn as_ptr_range(&self) -> Range<*const T> {
        let start = self.as_ptr();
        // SAFETY: the one-past-end pointer stays within the slice allocation.
        Range { start, end: unsafe { start.add(self.len()) } }
    }

    /// The half-open mutable pointer range, including the end pointer.
    pub fn as_mut_ptr_range(&mut self) -> Range<*mut T> {
        let start = self.as_mut_ptr();
        // SAFETY: the one-past-end pointer stays within the slice allocation.
        Range { start, end: unsafe { start.add(self.len()) } }
    }

    /// A mutable pointer to the first element.
    pub const fn as_mut_ptr(&mut self) -> *mut T {
        self as *mut [T] as *mut T
    }
}

/// An iterator over a slice's elements by mutable reference: what
/// [`[T]::iter_mut`] and `for x in &mut slice` give.
#[lang = "slice_iter_mut"]
pub struct IterMut<'a, T> {
    ptr: *mut T,
    start: usize,
    end: usize,
    marker: crate::marker::PhantomData<&'a mut T>,
}

// SAFETY: the iterator transfers a unique borrow of its remaining elements.
unsafe impl<T: Send> Send for IterMut<'_, T> {}
// SAFETY: shared iterator access exposes only shared references to T.
unsafe impl<T: Sync> Sync for IterMut<'_, T> {}

impl<'a, T> IterMut<'a, T> {
    /// A shared view of the elements that have not been yielded.
    pub fn as_slice(&self) -> &[T] {
        // SAFETY: only the remaining interval is shared. Previously yielded
        // mutable references are disjoint and may still be in use.
        unsafe { from_raw_parts(self.ptr.add(self.start), self.end - self.start) }
    }

    /// Consumes the iterator and returns its remaining unique slice.
    pub fn into_slice(self) -> &'a mut [T] {
        // SAFETY: yielded elements lie outside this remaining interval. The
        // iterator is consumed, so no later call can yield these elements.
        unsafe { from_raw_parts_mut(self.ptr.add(self.start), self.end - self.start) }
    }
}

impl<T: crate::fmt::Debug> crate::fmt::Debug for Iter<'_, T> {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        f.debug_tuple("Iter").field(&self.as_slice()).finish()
    }
}
impl<T: crate::fmt::Debug> crate::fmt::Debug for IterMut<'_, T> {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        f.debug_tuple("IterMut").field(&self.as_slice()).finish()
    }
}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;
    const __TRUSTED_LEN: bool = true;
    const __RANDOM_ACCESS: bool = true;
    const __MAY_HAVE_SIDE_EFFECT: bool = false;

    fn __iterator_get_unchecked(&mut self, idx: usize) -> &'a mut T {
        let i = self.start + idx;
        // SAFETY: the trusted random-access caller asks each index once.
        unsafe { &mut *self.ptr.add(i) }
    }

    fn next(&mut self) -> Option<&'a mut T> {
        if self.start < self.end {
            let i = self.start;
            self.start += 1;
            // SAFETY: advancing the initialized interval makes this element
            // unavailable to every future call, including next_back.
            Some(unsafe { &mut *self.ptr.add(i) })
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.end - self.start;
        (n, Some(n))
    }
}

impl<'a, T> ExactSizeIterator for IterMut<'a, T> {}
impl<'a, T> crate::iter::FusedIterator for IterMut<'a, T> {}

impl<'a, T> DoubleEndedIterator for IterMut<'a, T> {
    fn next_back(&mut self) -> Option<&'a mut T> {
        if self.start < self.end {
            self.end -= 1;
            let i = self.end;
            // SAFETY: shrinking the interval excludes this element from
            // every subsequent yield and remaining-slice view.
            Some(unsafe { &mut *self.ptr.add(i) })
        } else {
            None
        }
    }
}

impl<'a, T> IntoIterator for &'a mut [T] {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;

    fn into_iter(self) -> IterMut<'a, T> {
        self.iter_mut()
    }
}

/// The bounds of `slice[start..end]` for a slice of length `len`, which
/// panics as rustc's does where they are not within it.
#[track_caller]
fn checked_range(start: usize, end: usize, len: usize) -> Range<usize> {
    if start > end || end > len {
        slice_index_fail(start, end, len)
    }
    Range { start, end }
}

/// rustc's messages for a range not within a slice.
#[track_caller]
fn slice_index_fail(start: usize, end: usize, len: usize) -> ! {
    if start > len {
        crate::panicking::panic_fmt(format_args!(
            "range start index {start} out of range for slice of length {len}"
        ))
    }
    if end > len {
        crate::panicking::panic_fmt(format_args!(
            "range end index {end} out of range for slice of length {len}"
        ))
    }
    crate::panicking::panic_fmt(format_args!("slice index starts at {start} but ends at {end}"))
}

/// `..=end` as `..end + 1`, which panics where `end` is `usize::MAX`.
#[track_caller]
fn inclusive_end(end: usize) -> usize {
    if end == usize::MAX {
        panic("attempted to index slice up to maximum usize")
    }
    end + 1
}

impl<T, I: SliceIndex<[T]>> Index<I> for [T] {
    type Output = I::Output;
    #[track_caller]
    fn index(&self, index: I) -> &I::Output { SliceIndex::index(index, self) }
}

impl<T, I: SliceIndex<[T]>> IndexMut<I> for [T] {
    #[track_caller]
    fn index_mut(&mut self, index: I) -> &mut I::Output { SliceIndex::index_mut(index, self) }
}

/// What `get` and `get_mut` of a slice of `E`s take: a `usize`, for an
/// element, or a range of them, for a part. rustc's `SliceIndex<[E]>`.
#[diagnostic::on_unimplemented(message = "the type `{T}` cannot be indexed by `{Self}`")]
pub unsafe trait SliceIndex<T: ?Sized>: private_slice_index::Sealed {
    /// An element, or a slice.
    type Output: ?Sized;
    /// What it indexes, if it is within the slice.
    fn get(self, slice: &T) -> Option<&Self::Output>;
    /// What it indexes, mutably, if it is within the slice.
    fn get_mut(self, slice: &mut T) -> Option<&mut Self::Output>;
    fn index(self, slice: &T) -> &Self::Output;
    fn index_mut(self, slice: &mut T) -> &mut Self::Output;
}

mod private_slice_index {
    pub trait Sealed {}
    impl Sealed for usize {}
    impl Sealed for super::Range<usize> {}
    impl Sealed for super::RangeFrom<usize> {}
    impl Sealed for super::RangeTo<usize> {}
    impl Sealed for super::RangeFull {}
    impl Sealed for super::RangeInclusive<usize> {}
    impl Sealed for super::RangeToInclusive<usize> {}
    impl Sealed for (super::Bound<usize>, super::Bound<usize>) {}
}

unsafe impl<E> SliceIndex<[E]> for usize {
    type Output = E;

    fn get(self, slice: &[E]) -> Option<&E> {
        if self < slice.len() { Some(&slice[self]) } else { None }
    }

    fn get_mut(self, slice: &mut [E]) -> Option<&mut E> {
        if self < slice.len() { Some(&mut slice[self]) } else { None }
    }

    #[track_caller]
    fn index(self, slice: &[E]) -> &E { &slice[self] }
    #[track_caller]
    fn index_mut(self, slice: &mut [E]) -> &mut E { &mut slice[self] }
}

/// `slice[start..end]`, if it is within it.
fn get_range<E>(slice: &[E], start: usize, end: usize) -> Option<&[E]> {
    if start > end || end > slice.len() {
        None
    } else {
        Some(unsafe { subslice(slice, start, end) })
    }
}

/// `slice[start..end]`, mutably, if it is within it.
fn get_range_mut<E>(slice: &mut [E], start: usize, end: usize) -> Option<&mut [E]> {
    if start > end || end > slice.len() {
        None
    } else {
        Some(unsafe { subslice_mut(slice, start, end) })
    }
}

unsafe impl<E> SliceIndex<[E]> for Range<usize> {
    type Output = [E];

    fn get(self, slice: &[E]) -> Option<&[E]> {
        get_range(slice, self.start, self.end)
    }

    fn get_mut(self, slice: &mut [E]) -> Option<&mut [E]> {
        get_range_mut(slice, self.start, self.end)
    }

    #[track_caller]
    fn index(self, slice: &[E]) -> &[E] {
        let r = checked_range(self.start, self.end, slice.len());
        unsafe { subslice(slice, r.start, r.end) }
    }
    #[track_caller]
    fn index_mut(self, slice: &mut [E]) -> &mut [E] {
        let r = checked_range(self.start, self.end, slice.len());
        unsafe { subslice_mut(slice, r.start, r.end) }
    }
}

unsafe impl<E> SliceIndex<[E]> for RangeTo<usize> {
    type Output = [E];

    fn get(self, slice: &[E]) -> Option<&[E]> {
        get_range(slice, 0, self.end)
    }

    fn get_mut(self, slice: &mut [E]) -> Option<&mut [E]> {
        get_range_mut(slice, 0, self.end)
    }

    #[track_caller]
    fn index(self, slice: &[E]) -> &[E] {
        let r = checked_range(0, self.end, slice.len());
        unsafe { subslice(slice, r.start, r.end) }
    }
    #[track_caller]
    fn index_mut(self, slice: &mut [E]) -> &mut [E] {
        let r = checked_range(0, self.end, slice.len());
        unsafe { subslice_mut(slice, r.start, r.end) }
    }
}

unsafe impl<E> SliceIndex<[E]> for RangeFrom<usize> {
    type Output = [E];

    fn get(self, slice: &[E]) -> Option<&[E]> {
        let len = slice.len();
        get_range(slice, self.start, len)
    }

    fn get_mut(self, slice: &mut [E]) -> Option<&mut [E]> {
        let len = slice.len();
        get_range_mut(slice, self.start, len)
    }

    #[track_caller]
    fn index(self, slice: &[E]) -> &[E] {
        let len = slice.len();
        if self.start > len {
            slice_index_fail(self.start, len, len)
        }
        unsafe { subslice(slice, self.start, len) }
    }
    #[track_caller]
    fn index_mut(self, slice: &mut [E]) -> &mut [E] {
        let len = slice.len();
        if self.start > len {
            slice_index_fail(self.start, len, len)
        }
        unsafe { subslice_mut(slice, self.start, len) }
    }
}

unsafe impl<E> SliceIndex<[E]> for RangeFull {
    type Output = [E];

    fn get(self, slice: &[E]) -> Option<&[E]> {
        Some(slice)
    }

    fn get_mut(self, slice: &mut [E]) -> Option<&mut [E]> {
        Some(slice)
    }

    #[track_caller]
    fn index(self, slice: &[E]) -> &[E] {
        slice
    }
    #[track_caller]
    fn index_mut(self, slice: &mut [E]) -> &mut [E] {
        slice
    }
}

unsafe impl<E> SliceIndex<[E]> for RangeInclusive<usize> {
    type Output = [E];

    fn get(self, slice: &[E]) -> Option<&[E]> {
        if self.end == usize::MAX {
            return None;
        }
        let end = self.end + 1;
        let start = if self.exhausted { end } else { self.start };
        get_range(slice, start, end)
    }

    fn get_mut(self, slice: &mut [E]) -> Option<&mut [E]> {
        if self.end == usize::MAX {
            return None;
        }
        let end = self.end + 1;
        let start = if self.exhausted { end } else { self.start };
        get_range_mut(slice, start, end)
    }

    #[track_caller]
    fn index(self, slice: &[E]) -> &[E] {
        let end = inclusive_end(self.end);
        let start = if self.exhausted { end } else { self.start };
        let r = checked_range(start, end, slice.len());
        unsafe { subslice(slice, r.start, r.end) }
    }
    #[track_caller]
    fn index_mut(self, slice: &mut [E]) -> &mut [E] {
        let end = inclusive_end(self.end);
        let start = if self.exhausted { end } else { self.start };
        let r = checked_range(start, end, slice.len());
        unsafe { subslice_mut(slice, r.start, r.end) }
    }
}

unsafe impl<E> SliceIndex<[E]> for RangeToInclusive<usize> {
    type Output = [E];

    fn get(self, slice: &[E]) -> Option<&[E]> {
        if self.end == usize::MAX { None } else { get_range(slice, 0, self.end + 1) }
    }

    fn get_mut(self, slice: &mut [E]) -> Option<&mut [E]> {
        if self.end == usize::MAX { None } else { get_range_mut(slice, 0, self.end + 1) }
    }

    #[track_caller]
    fn index(self, slice: &[E]) -> &[E] {
        let r = checked_range(0, inclusive_end(self.end), slice.len());
        unsafe { subslice(slice, r.start, r.end) }
    }
    #[track_caller]
    fn index_mut(self, slice: &mut [E]) -> &mut [E] {
        let r = checked_range(0, inclusive_end(self.end), slice.len());
        unsafe { subslice_mut(slice, r.start, r.end) }
    }
}

/// The bounds `range` gives for a slice of length `bounds.end`,
/// panicking as rustc's (unstable) `slice::range` does where they are
/// not within it: what `Vec::drain` takes.
#[track_caller]
pub fn range<R: RangeBounds<usize>>(range: R, bounds: RangeTo<usize>) -> Range<usize> {
    bounds_range(range, bounds.end)
}

/// The bounds `range` gives for a slice of length `len`, which panics
/// as rustc's `slice::range` does where they are not within it.
#[track_caller]
fn bounds_range<R: RangeBounds<usize>>(range: R, len: usize) -> Range<usize> {
    let start = match range.start_bound() {
        Bound::Included(&start) => start,
        Bound::Excluded(&start) => {
            if start == usize::MAX {
                panic("attempted to index slice from after maximum usize")
            }
            start + 1
        }
        Bound::Unbounded => 0,
    };
    let end = match range.end_bound() {
        Bound::Included(&end) => inclusive_end(end),
        Bound::Excluded(&end) => end,
        Bound::Unbounded => len,
    };
    checked_range(start, end, len)
}

/// [`[T]::chunks_exact_mut`]'s iterator.
pub struct ChunksExactMut<'a, T> {
    v: &'a mut [T],
    start: usize,
    end: usize,
    rem: &'a mut [T],
    chunk_size: usize,
}

impl<'a, T> ChunksExactMut<'a, T> {
    /// The elements left over, fewer than a chunk.
    pub fn into_remainder(self) -> &'a mut [T] {
        self.rem
    }
}

impl<'a, T> Iterator for ChunksExactMut<'a, T> {
    type Item = &'a mut [T];

    fn next(&mut self) -> Option<&'a mut [T]> {
        if self.end - self.start < self.chunk_size {
            None
        } else {
            let start = self.start;
            self.start += self.chunk_size;
            // Chunks given are not given again.
            Some(unsafe { subslice_mut(self.v, start, start + self.chunk_size) })
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = (self.end - self.start) / self.chunk_size;
        (n, Some(n))
    }
}

impl<'a, T> DoubleEndedIterator for ChunksExactMut<'a, T> {
    fn next_back(&mut self) -> Option<&'a mut [T]> {
        if self.end - self.start < self.chunk_size {
            None
        } else {
            let end = self.end;
            self.end -= self.chunk_size;
            Some(unsafe { subslice_mut(self.v, end - self.chunk_size, end) })
        }
    }
}

impl<'a, T> ExactSizeIterator for ChunksExactMut<'a, T> {}

/// [`[T]::rchunks`]'s iterator.
pub struct RChunks<'a, T> {
    v: &'a [T],
    chunk_size: usize,
}

impl<'a, T> Iterator for RChunks<'a, T> {
    type Item = &'a [T];

    fn next(&mut self) -> Option<&'a [T]> {
        let len = self.v.len();
        if len == 0 {
            None
        } else {
            let n = if len < self.chunk_size { len } else { self.chunk_size };
            let chunk = unsafe { subslice(self.v, len - n, len) };
            self.v = unsafe { subslice(self.v, 0, len - n) };
            Some(chunk)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.v.len();
        let n = len / self.chunk_size + if len % self.chunk_size != 0 { 1 } else { 0 };
        (n, Some(n))
    }
}

impl<'a, T> DoubleEndedIterator for RChunks<'a, T> {
    fn next_back(&mut self) -> Option<&'a [T]> {
        let len = self.v.len();
        if len == 0 {
            None
        } else {
            let remainder = len % self.chunk_size;
            let size = if remainder != 0 { remainder } else { self.chunk_size };
            let chunk = unsafe { subslice(self.v, 0, size) };
            self.v = unsafe { subslice(self.v, size, len) };
            Some(chunk)
        }
    }
}

impl<'a, T> ExactSizeIterator for RChunks<'a, T> {}

impl<'a, T> Clone for RChunks<'a, T> {
    fn clone(&self) -> RChunks<'a, T> {
        RChunks { v: self.v, chunk_size: self.chunk_size }
    }
}

/// [`[T]::rchunks_exact`]'s iterator.
pub struct RChunksExact<'a, T> {
    v: &'a [T],
    rem: &'a [T],
    chunk_size: usize,
}

impl<'a, T> RChunksExact<'a, T> {
    /// The elements left over at the start, fewer than a chunk.
    pub fn remainder(&self) -> &'a [T] {
        self.rem
    }
}

impl<'a, T> Iterator for RChunksExact<'a, T> {
    type Item = &'a [T];

    fn next(&mut self) -> Option<&'a [T]> {
        let len = self.v.len();
        if len < self.chunk_size {
            None
        } else {
            let chunk = unsafe { subslice(self.v, len - self.chunk_size, len) };
            self.v = unsafe { subslice(self.v, 0, len - self.chunk_size) };
            Some(chunk)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.v.len() / self.chunk_size;
        (n, Some(n))
    }
}

impl<'a, T> DoubleEndedIterator for RChunksExact<'a, T> {
    fn next_back(&mut self) -> Option<&'a [T]> {
        let len = self.v.len();
        if len < self.chunk_size {
            None
        } else {
            let chunk = unsafe { subslice(self.v, 0, self.chunk_size) };
            self.v = unsafe { subslice(self.v, self.chunk_size, len) };
            Some(chunk)
        }
    }
}

impl<'a, T> ExactSizeIterator for RChunksExact<'a, T> {}

impl<'a, T> Clone for RChunksExact<'a, T> {
    fn clone(&self) -> RChunksExact<'a, T> {
        RChunksExact { v: self.v, rem: self.rem, chunk_size: self.chunk_size }
    }
}

/// [`[T]::windows`]'s iterator.
pub struct Windows<'a, T> {
    v: &'a [T],
    size: usize,
}

impl<'a, T> Iterator for Windows<'a, T> {
    type Item = &'a [T];
    const __TRUSTED_LEN: bool = true;
    const __RANDOM_ACCESS: bool = true;
    const __MAY_HAVE_SIDE_EFFECT: bool = false;

    fn __iterator_get_unchecked(&mut self, idx: usize) -> &'a [T] {
        unsafe { subslice(self.v, idx, idx + self.size) }
    }

    fn next(&mut self) -> Option<&'a [T]> {
        if self.size > self.v.len() {
            None
        } else {
            let window = unsafe { subslice(self.v, 0, self.size) };
            self.v = unsafe { subslice(self.v, 1, self.v.len()) };
            Some(window)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = if self.size > self.v.len() { 0 } else { self.v.len() - self.size + 1 };
        (n, Some(n))
    }
}

impl<'a, T> DoubleEndedIterator for Windows<'a, T> {
    fn next_back(&mut self) -> Option<&'a [T]> {
        let len = self.v.len();
        if self.size > len {
            None
        } else {
            let window = unsafe { subslice(self.v, len - self.size, len) };
            self.v = unsafe { subslice(self.v, 0, len - 1) };
            Some(window)
        }
    }
}

impl<'a, T> ExactSizeIterator for Windows<'a, T> {}

/// [`[T]::array_windows`]'s overlapping fixed-size windows.
pub struct ArrayWindows<'a, T, const N: usize> {
    inner: Windows<'a, T>,
}

impl<'a, T, const N: usize> Clone for ArrayWindows<'a, T, N> {
    fn clone(&self) -> Self {
        Self { inner: Windows { v: self.inner.v, size: N } }
    }
}

impl<'a, T, const N: usize> Iterator for ArrayWindows<'a, T, N> {
    type Item = &'a [T; N];

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().and_then(|window| window.first_chunk::<N>())
    }

    fn size_hint(&self) -> (usize, Option<usize>) { self.inner.size_hint() }
}

impl<'a, T, const N: usize> DoubleEndedIterator for ArrayWindows<'a, T, N> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back().and_then(|window| window.first_chunk::<N>())
    }
}

impl<T, const N: usize> ExactSizeIterator for ArrayWindows<'_, T, N> {}
impl<T, const N: usize> crate::iter::FusedIterator for ArrayWindows<'_, T, N> {}

/// [`[T]::chunks`]'s iterator.
pub struct Chunks<'a, T> {
    v: &'a [T],
    chunk_size: usize,
}

impl<'a, T> Iterator for Chunks<'a, T> {
    type Item = &'a [T];
    const __TRUSTED_LEN: bool = true;
    const __RANDOM_ACCESS: bool = true;
    const __MAY_HAVE_SIDE_EFFECT: bool = false;

    fn __iterator_get_unchecked(&mut self, idx: usize) -> &'a [T] {
        let start = idx * self.chunk_size;
        let end = crate::cmp::min(start + self.chunk_size, self.v.len());
        unsafe { subslice(self.v, start, end) }
    }

    fn next(&mut self) -> Option<&'a [T]> {
        let len = self.v.len();
        if len == 0 {
            None
        } else {
            let n = if len < self.chunk_size { len } else { self.chunk_size };
            let chunk = unsafe { subslice(self.v, 0, n) };
            self.v = unsafe { subslice(self.v, n, len) };
            Some(chunk)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.v.len();
        let n = len / self.chunk_size + if len % self.chunk_size != 0 { 1 } else { 0 };
        (n, Some(n))
    }
}

impl<'a, T> DoubleEndedIterator for Chunks<'a, T> {
    fn next_back(&mut self) -> Option<&'a [T]> {
        let len = self.v.len();
        if len == 0 {
            None
        } else {
            let remainder = len % self.chunk_size;
            let size = if remainder != 0 { remainder } else { self.chunk_size };
            let chunk = unsafe { subslice(self.v, len - size, len) };
            self.v = unsafe { subslice(self.v, 0, len - size) };
            Some(chunk)
        }
    }
}

impl<'a, T> ExactSizeIterator for Chunks<'a, T> {}

/// [`[T]::chunks_exact`]'s iterator.
pub struct ChunksExact<'a, T> {
    v: &'a [T],
    rem: &'a [T],
    chunk_size: usize,
}

impl<'a, T> ChunksExact<'a, T> {
    /// The elements left over, fewer than a chunk.
    pub fn remainder(&self) -> &'a [T] {
        self.rem
    }
}

impl<'a, T> Iterator for ChunksExact<'a, T> {
    type Item = &'a [T];
    const __TRUSTED_LEN: bool = true;
    const __RANDOM_ACCESS: bool = true;
    const __MAY_HAVE_SIDE_EFFECT: bool = false;

    fn __iterator_get_unchecked(&mut self, idx: usize) -> &'a [T] {
        let start = idx * self.chunk_size;
        unsafe { subslice(self.v, start, start + self.chunk_size) }
    }

    fn next(&mut self) -> Option<&'a [T]> {
        let len = self.v.len();
        if len < self.chunk_size {
            None
        } else {
            let chunk = unsafe { subslice(self.v, 0, self.chunk_size) };
            self.v = unsafe { subslice(self.v, self.chunk_size, len) };
            Some(chunk)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.v.len() / self.chunk_size;
        (n, Some(n))
    }
}

impl<'a, T> DoubleEndedIterator for ChunksExact<'a, T> {
    fn next_back(&mut self) -> Option<&'a [T]> {
        let len = self.v.len();
        if len < self.chunk_size {
            None
        } else {
            let chunk = unsafe { subslice(self.v, len - self.chunk_size, len) };
            self.v = unsafe { subslice(self.v, 0, len - self.chunk_size) };
            Some(chunk)
        }
    }
}

impl<'a, T> ExactSizeIterator for ChunksExact<'a, T> {}

/// [`[T]::chunks_mut`]'s iterator.
pub struct ChunksMut<'a, T> {
    v: &'a mut [T],
    start: usize,
    end: usize,
    chunk_size: usize,
}

impl<'a, T> Iterator for ChunksMut<'a, T> {
    type Item = &'a mut [T];
    const __TRUSTED_LEN: bool = true;
    const __RANDOM_ACCESS: bool = true;
    const __MAY_HAVE_SIDE_EFFECT: bool = false;

    fn __iterator_get_unchecked(&mut self, idx: usize) -> &'a mut [T] {
        let start = self.start + idx * self.chunk_size;
        let end = crate::cmp::min(start + self.chunk_size, self.end);
        // Each index is asked for once.
        unsafe { subslice_mut(self.v, start, end) }
    }

    fn next(&mut self) -> Option<&'a mut [T]> {
        if self.start == self.end {
            None
        } else {
            let left = self.end - self.start;
            let n = if left < self.chunk_size { left } else { self.chunk_size };
            let start = self.start;
            self.start += n;
            // Chunks given are not given again.
            Some(unsafe { subslice_mut(self.v, start, start + n) })
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.end - self.start;
        let n = len / self.chunk_size + if len % self.chunk_size != 0 { 1 } else { 0 };
        (n, Some(n))
    }
}

impl<'a, T> DoubleEndedIterator for ChunksMut<'a, T> {
    fn next_back(&mut self) -> Option<&'a mut [T]> {
        if self.start == self.end {
            None
        } else {
            let len = self.end - self.start;
            let remainder = len % self.chunk_size;
            let size = if remainder != 0 { remainder } else { self.chunk_size };
            let end = self.end;
            self.end -= size;
            Some(unsafe { subslice_mut(self.v, end - size, end) })
        }
    }
}

impl<'a, T> ExactSizeIterator for ChunksMut<'a, T> {}

/// Slices are equal if they are as long and their elements are equal in
/// turn.
impl<A: PartialEq<B>, B> PartialEq<[B]> for [A] {
    fn eq(&self, other: &[B]) -> bool {
        if self.len() != other.len() {
            return false;
        }
        let mut i = 0;
        while i < self.len() {
            if self[i] != other[i] {
                return false;
            }
            i += 1;
        }
        true
    }
}

impl<T: Eq> Eq for [T] {}

/// Slices are ordered by their first elements that differ, and a slice
/// before those it begins.
impl<T: PartialOrd> PartialOrd for [T] {
    fn partial_cmp(&self, other: &[T]) -> Option<Ordering> {
        let l = if self.len() < other.len() { self.len() } else { other.len() };
        let mut i = 0;
        while i < l {
            match self[i].partial_cmp(&other[i]) {
                Some(Equal) => {}
                non_eq => return non_eq,
            }
            i += 1;
        }
        self.len().partial_cmp(&other.len())
    }
}

impl<T: Ord> Ord for [T] {
    fn cmp(&self, other: &[T]) -> Ordering {
        let l = if self.len() < other.len() { self.len() } else { other.len() };
        let mut i = 0;
        while i < l {
            match self[i].cmp(&other[i]) {
                Equal => {}
                non_eq => return non_eq,
            }
            i += 1;
        }
        self.len().cmp(&other.len())
    }
}

/// rustc's binary search: halves the range without stopping at an equal
/// element, so it finds the same one of several.
fn binary_search<'a, T, F: FnMut(&'a T) -> Ordering>(v: &'a [T], f: &mut F) -> Result<usize, usize> {
    let mut size = v.len();
    if size == 0 {
        return Err(0);
    }
    let mut base = 0;
    while size > 1 {
        let half = size / 2;
        let mid = base + half;
        let cmp = (*f)(&v[mid]);
        base = if cmp == Greater { base } else { mid };
        size -= half;
    }
    let cmp = (*f)(&v[base]);
    if cmp == Equal {
        Ok(base)
    } else {
        Err(base + (cmp == Less) as usize)
    }
}
#[derive(Clone)]
pub struct EscapeAscii<'a> {
    bytes: &'a [u8],
    index: usize,
    pending: Option<crate::ascii::EscapeDefault>,
}
impl<'a> EscapeAscii<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self { Self { bytes, index: 0, pending: None } }
}
impl Iterator for EscapeAscii<'_> {
    type Item = u8;
    fn next(&mut self) -> Option<u8> {
        loop {
            if let Some(pending) = &mut self.pending {
                if let Some(byte) = pending.next() { return Some(byte); }
            }
            if self.index == self.bytes.len() { self.pending = None; return None; }
            self.pending = Some(crate::ascii::escape_default(self.bytes[self.index]));
            self.index += 1;
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let pending = self.pending.as_ref().map_or(0, |value| value.len());
        let remaining = self.bytes.len() - self.index;
        (remaining + pending, remaining.checked_mul(4).and_then(|max| max.checked_add(pending)))
    }
}
impl crate::iter::FusedIterator for EscapeAscii<'_> {}
impl crate::fmt::Display for EscapeAscii<'_> {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        for byte in self.clone() { crate::write!(f, "{}", byte as char)?; } Ok(())
    }
}


// SAFETY: every method checks both endpoint arithmetic and slice bounds before
// constructing a view. Mutable results retain the unique input borrow.
unsafe impl<E> SliceIndex<[E]> for (Bound<usize>, Bound<usize>) {
    type Output = [E];
    fn get(self, slice: &[E]) -> Option<&[E]> {
        let range = tuple_range_checked(self, slice.len())?;
        get_range(slice, range.start, range.end)
    }
    fn get_mut(self, slice: &mut [E]) -> Option<&mut [E]> {
        let range = tuple_range_checked(self, slice.len())?;
        get_range_mut(slice, range.start, range.end)
    }
    #[track_caller]
    fn index(self, slice: &[E]) -> &[E] {
        let range = bounds_range(self, slice.len());
        // SAFETY: bounds_range checked both endpoints and their order.
        unsafe { subslice(slice, range.start, range.end) }
    }
    #[track_caller]
    fn index_mut(self, slice: &mut [E]) -> &mut [E] {
        let range = bounds_range(self, slice.len());
        // SAFETY: bounds_range checked both endpoints and their order.
        unsafe { subslice_mut(slice, range.start, range.end) }
    }
}

fn tuple_range_checked(bounds: (Bound<usize>, Bound<usize>), len: usize) -> Option<Range<usize>> {
    let start = match bounds.0 {
        Bound::Included(value) => value,
        Bound::Excluded(value) => value.checked_add(1)?,
        Bound::Unbounded => 0,
    };
    let end = match bounds.1 {
        Bound::Included(value) => value.checked_add(1)?,
        Bound::Excluded(value) => value,
        Bound::Unbounded => len,
    };
    if start > end || end > len { None } else { Some(Range { start, end }) }
}

/// Shared pieces separated by elements accepted by a predicate.
pub struct Split<'a, T, P: FnMut(&T) -> bool> { slice: &'a [T], pred: P, finished: bool }
impl<'a, T, P: FnMut(&T) -> bool> Iterator for Split<'a, T, P> {
    type Item = &'a [T];
    fn next(&mut self) -> Option<&'a [T]> {
        if self.finished { return None; }
        let mut i = 0;
        while i < self.slice.len() {
            if (self.pred)(&self.slice[i]) {
                let result = &self.slice[..i];
                self.slice = &self.slice[i + 1..];
                return Some(result);
            }
            i += 1;
        }
        self.finished = true;
        Some(self.slice)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.finished { (0, Some(0)) } else { (1, self.slice.len().checked_add(1)) }
    }
}
impl<'a, T, P: FnMut(&T) -> bool> DoubleEndedIterator for Split<'a, T, P> {
    fn next_back(&mut self) -> Option<&'a [T]> {
        if self.finished { return None; }
        let mut i = self.slice.len();
        while i > 0 {
            i -= 1;
            if (self.pred)(&self.slice[i]) {
                let result = &self.slice[i + 1..];
                self.slice = &self.slice[..i];
                return Some(result);
            }
        }
        self.finished = true;
        Some(self.slice)
    }
}
impl<T, P: FnMut(&T) -> bool> crate::iter::FusedIterator for Split<'_, T, P> {}
impl<T, P: Clone + FnMut(&T) -> bool> Clone for Split<'_, T, P> {
    fn clone(&self) -> Self { Split { slice: self.slice, pred: self.pred.clone(), finished: self.finished } }
}

/// Pieces of a slice, each ending with an element the predicate accepts.
pub struct SplitInclusive<'a, T, P: FnMut(&T) -> bool> { slice: &'a [T], pred: P, finished: bool }
impl<'a, T, P: FnMut(&T) -> bool> Iterator for SplitInclusive<'a, T, P> {
    type Item = &'a [T];
    fn next(&mut self) -> Option<&'a [T]> {
        if self.finished { return None; }
        let mut end = self.slice.len();
        let mut i = 0;
        while i < self.slice.len() {
            if (self.pred)(&self.slice[i]) { end = i + 1; break; }
            i += 1;
        }
        if end == self.slice.len() { self.finished = true; }
        let result = &self.slice[..end];
        self.slice = &self.slice[end..];
        Some(result)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.finished { (0, Some(0)) } else { (1, Some(crate::cmp::max(1, self.slice.len()))) }
    }
}
impl<'a, T, P: FnMut(&T) -> bool> DoubleEndedIterator for SplitInclusive<'a, T, P> {
    fn next_back(&mut self) -> Option<&'a [T]> {
        if self.finished { return None; }
        // The last element ends the final piece whether or not it matches.
        let search = if self.slice.is_empty() { 0 } else { self.slice.len() - 1 };
        let mut start = 0;
        let mut i = search;
        while i > 0 {
            i -= 1;
            if (self.pred)(&self.slice[i]) { start = i + 1; break; }
        }
        if start == 0 { self.finished = true; }
        let result = &self.slice[start..];
        self.slice = &self.slice[..start];
        Some(result)
    }
}
impl<T, P: FnMut(&T) -> bool> crate::iter::FusedIterator for SplitInclusive<'_, T, P> {}

/// At most `count` pieces of a slice split from the front.
pub struct SplitN<'a, T, P: FnMut(&T) -> bool> { inner: Split<'a, T, P>, count: usize }
impl<'a, T, P: FnMut(&T) -> bool> Iterator for SplitN<'a, T, P> {
    type Item = &'a [T];
    fn next(&mut self) -> Option<&'a [T]> {
        match self.count {
            0 => None,
            1 => { self.count -= 1; if self.inner.finished { None } else { self.inner.finished = true; Some(self.inner.slice) } }
            _ => { self.count -= 1; self.inner.next() }
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lower, upper) = self.inner.size_hint();
        let count = self.count;
        (crate::cmp::min(lower, count), Some(upper.map_or(count, |upper| crate::cmp::min(count, upper))))
    }
}
impl<T, P: FnMut(&T) -> bool> crate::iter::FusedIterator for SplitN<'_, T, P> {}

/// Pieces of a slice split from the back.
pub struct RSplit<'a, T, P: FnMut(&T) -> bool> { inner: Split<'a, T, P> }
impl<'a, T, P: FnMut(&T) -> bool> Iterator for RSplit<'a, T, P> {
    type Item = &'a [T];
    fn next(&mut self) -> Option<&'a [T]> { self.inner.next_back() }
    fn size_hint(&self) -> (usize, Option<usize>) { self.inner.size_hint() }
}
impl<'a, T, P: FnMut(&T) -> bool> DoubleEndedIterator for RSplit<'a, T, P> {
    fn next_back(&mut self) -> Option<&'a [T]> { self.inner.next() }
}
impl<T, P: FnMut(&T) -> bool> crate::iter::FusedIterator for RSplit<'_, T, P> {}

/// At most `count` pieces of a slice split from the back.
pub struct RSplitN<'a, T, P: FnMut(&T) -> bool> { inner: Split<'a, T, P>, count: usize }
impl<'a, T, P: FnMut(&T) -> bool> Iterator for RSplitN<'a, T, P> {
    type Item = &'a [T];
    fn next(&mut self) -> Option<&'a [T]> {
        match self.count {
            0 => None,
            1 => { self.count -= 1; if self.inner.finished { None } else { self.inner.finished = true; Some(self.inner.slice) } }
            _ => { self.count -= 1; self.inner.next_back() }
        }
    }
}
impl<T, P: FnMut(&T) -> bool> crate::iter::FusedIterator for RSplitN<'_, T, P> {}

/// Mutable pieces of a slice between accepted elements.
pub struct SplitMut<'a, T, P: FnMut(&T) -> bool> { slice: &'a mut [T], pred: P, finished: bool }
impl<'a, T, P: FnMut(&T) -> bool> Iterator for SplitMut<'a, T, P> {
    type Item = &'a mut [T];
    fn next(&mut self) -> Option<&'a mut [T]> {
        if self.finished { return None; }
        let mut index = None;
        let mut i = 0;
        while i < self.slice.len() {
            if (self.pred)(&self.slice[i]) { index = Some(i); break; }
            i += 1;
        }
        let slice = crate::mem::take(&mut self.slice);
        match index {
            Some(i) => {
                let (head, tail) = slice.split_at_mut(i);
                self.slice = &mut tail[1..];
                Some(head)
            }
            None => { self.finished = true; Some(slice) }
        }
    }
}
impl<T, P: FnMut(&T) -> bool> crate::iter::FusedIterator for SplitMut<'_, T, P> {}

/// Runs of adjacent elements that a predicate accepts pairwise.
pub struct ChunkBy<'a, T, P> { slice: &'a [T], predicate: P }
impl<'a, T, P: FnMut(&T, &T) -> bool> Iterator for ChunkBy<'a, T, P> {
    type Item = &'a [T];
    fn next(&mut self) -> Option<&'a [T]> {
        if self.slice.is_empty() { return None; }
        let mut len = 1;
        while len < self.slice.len() && (self.predicate)(&self.slice[len - 1], &self.slice[len]) { len += 1; }
        let (head, tail) = self.slice.split_at(len);
        self.slice = tail;
        Some(head)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.slice.is_empty() { (0, Some(0)) } else { (1, Some(self.slice.len())) }
    }
}
impl<'a, T, P: FnMut(&T, &T) -> bool> DoubleEndedIterator for ChunkBy<'a, T, P> {
    fn next_back(&mut self) -> Option<&'a [T]> {
        if self.slice.is_empty() { return None; }
        let mut len = 1;
        let n = self.slice.len();
        while len < n && (self.predicate)(&self.slice[n - len - 1], &self.slice[n - len]) { len += 1; }
        let (head, tail) = self.slice.split_at(n - len);
        self.slice = head;
        Some(tail)
    }
}
impl<T, P: FnMut(&T, &T) -> bool> crate::iter::FusedIterator for ChunkBy<'_, T, P> {}

impl<T, const N: usize> [[T; N]] {
    /// The elements of the arrays, in order, as one slice.
    pub const fn as_flattened(&self) -> &[T] {
        let len = match self.len().checked_mul(N) {
            Some(len) => len,
            None => panic("slice len overflow"),
        };
        // SAFETY: `[T; N]` has no padding between elements, so the arrays
        // are `len` contiguous `T`s borrowed for as long as `self`.
        unsafe { from_raw_parts(self.as_ptr() as *const T, len) }
    }

    /// The elements of the arrays, in order, as one mutable slice.
    pub const fn as_flattened_mut(&mut self) -> &mut [T] {
        let len = match self.len().checked_mul(N) {
            Some(len) => len,
            None => panic("slice len overflow"),
        };
        // SAFETY: as in `as_flattened`, with the unique borrow of `self`.
        unsafe { from_raw_parts_mut(self.as_mut_ptr() as *mut T, len) }
    }
}

/// Mutable chunks yielded from the end of a slice.
pub struct RChunksMut<'a, T> { slice: &'a mut [T], chunk_size: usize }
impl<'a, T> Iterator for RChunksMut<'a, T> {
    type Item = &'a mut [T];
    fn next(&mut self) -> Option<Self::Item> {
        if self.slice.is_empty() { return None; }
        let index = self.slice.len().saturating_sub(self.chunk_size);
        let (head, tail) = crate::mem::take(&mut self.slice).split_at_mut(index);
        self.slice = head;
        Some(tail)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.slice.len();
        let count = len / self.chunk_size + if len % self.chunk_size != 0 { 1 } else { 0 };
        (count, Some(count))
    }
}
impl<'a, T> DoubleEndedIterator for RChunksMut<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.slice.is_empty() { return None; }
        let rem = self.slice.len() % self.chunk_size;
        let count = if rem == 0 { self.chunk_size } else { rem };
        let (head, tail) = crate::mem::take(&mut self.slice).split_at_mut(count);
        self.slice = tail;
        Some(head)
    }
}
impl<T> ExactSizeIterator for RChunksMut<'_, T> {}
impl<T> crate::iter::FusedIterator for RChunksMut<'_, T> {}

/// Exact mutable chunks yielded from the end of a slice.
pub struct RChunksExactMut<'a, T> { inner: ChunksExactMut<'a, T>, rem: &'a mut [T] }
impl<'a, T> RChunksExactMut<'a, T> {
    /// The initial elements that do not fill a chunk.
    pub fn into_remainder(self) -> &'a mut [T] { self.rem }
}
impl<'a, T> Iterator for RChunksExactMut<'a, T> {
    type Item = &'a mut [T];
    fn next(&mut self) -> Option<Self::Item> { self.inner.next_back() }
    fn size_hint(&self) -> (usize, Option<usize>) { self.inner.size_hint() }
}
impl<'a, T> DoubleEndedIterator for RChunksExactMut<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> { self.inner.next() }
}
impl<T> ExactSizeIterator for RChunksExactMut<'_, T> {}
impl<T> crate::iter::FusedIterator for RChunksExactMut<'_, T> {}

impl<'a, T, P: FnMut(&T) -> bool> DoubleEndedIterator for SplitMut<'a, T, P> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.finished { return None; }
        let mut index = None;
        let mut i = self.slice.len();
        while i > 0 { i -= 1; if (self.pred)(&self.slice[i]) { index = Some(i); break; } }
        let slice = crate::mem::take(&mut self.slice);
        match index {
            Some(i) => { let (head, tail) = slice.split_at_mut(i); self.slice = head; Some(&mut tail[1..]) }
            None => { self.finished = true; Some(slice) }
        }
    }
}

/// Mutable pieces ending with an element accepted by the predicate.
pub struct SplitInclusiveMut<'a, T, P: FnMut(&T) -> bool> { slice: &'a mut [T], pred: P, finished: bool }
impl<'a, T, P: FnMut(&T) -> bool> Iterator for SplitInclusiveMut<'a, T, P> {
    type Item = &'a mut [T];
    fn next(&mut self) -> Option<Self::Item> {
        if self.finished { return None; }
        let mut end = self.slice.len();
        let mut i = 0;
        while i < self.slice.len() { if (self.pred)(&self.slice[i]) { end = i + 1; break; } i += 1; }
        if end == self.slice.len() { self.finished = true; }
        let (head, tail) = crate::mem::take(&mut self.slice).split_at_mut(end);
        self.slice = tail;
        Some(head)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.finished { (0, Some(0)) } else { (1, Some(self.slice.len())) }
    }
}
impl<'a, T, P: FnMut(&T) -> bool> DoubleEndedIterator for SplitInclusiveMut<'a, T, P> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.finished { return None; }
        let mut start = 0;
        let mut i = self.slice.len().saturating_sub(1);
        while i > 0 { i -= 1; if (self.pred)(&self.slice[i]) { start = i + 1; break; } }
        if start == 0 { self.finished = true; }
        let (head, tail) = crate::mem::take(&mut self.slice).split_at_mut(start);
        self.slice = head;
        Some(tail)
    }
}
impl<T, P: FnMut(&T) -> bool> crate::iter::FusedIterator for SplitInclusiveMut<'_, T, P> {}

/// Mutable runs accepted pairwise by a predicate.
pub struct ChunkByMut<'a, T, P> { slice: &'a mut [T], predicate: P }
impl<'a, T, P: FnMut(&T, &T) -> bool> Iterator for ChunkByMut<'a, T, P> {
    type Item = &'a mut [T];
    fn next(&mut self) -> Option<Self::Item> {
        if self.slice.is_empty() { return None; }
        let mut len = 1;
        while len < self.slice.len() && (self.predicate)(&self.slice[len - 1], &self.slice[len]) { len += 1; }
        let (head, tail) = crate::mem::take(&mut self.slice).split_at_mut(len);
        self.slice = tail;
        Some(head)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.slice.is_empty() { (0, Some(0)) } else { (1, Some(self.slice.len())) }
    }
}
impl<'a, T, P: FnMut(&T, &T) -> bool> DoubleEndedIterator for ChunkByMut<'a, T, P> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.slice.is_empty() { return None; }
        let mut start = self.slice.len() - 1;
        while start > 0 && (self.predicate)(&self.slice[start - 1], &self.slice[start]) { start -= 1; }
        let (head, tail) = crate::mem::take(&mut self.slice).split_at_mut(start);
        self.slice = head;
        Some(tail)
    }
}
impl<T, P: FnMut(&T, &T) -> bool> crate::iter::FusedIterator for ChunkByMut<'_, T, P> {}
