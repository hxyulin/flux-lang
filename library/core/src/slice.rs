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

impl<T> [T] {
    /// The first element, or `None` if the slice is empty.
    pub fn first(&self) -> Option<&T> {
        if self.len() == 0 { None } else { Some(&self[0]) }
    }

    /// The last element, or `None` if the slice is empty.
    pub fn last(&self) -> Option<&T> {
        let len = self.len();
        if len == 0 { None } else { Some(&self[len - 1]) }
    }

    /// The element at `index`, or the part a range of indices gives, or
    /// `None` if it is out of bounds.
    pub fn get<I: SliceIndex<T>>(&self, index: I) -> Option<&I::Output> {
        index.get(self)
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
            slice: self,
            start: 0,
            end,
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
    pub fn get_mut<I: SliceIndex<T>>(&mut self, index: I) -> Option<&mut I::Output> {
        index.get_mut(self)
    }

    /// The first element and the rest, or `None` if the slice is empty.
    pub fn split_first(&self) -> Option<(&T, &[T])> {
        if self.len() == 0 { None } else { Some((&self[0], &self[1..])) }
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
    pub fn split_at(&self, mid: usize) -> (&[T], &[T]) {
        let len = self.len();
        if mid > len {
            panic("mid > len")
        }
        unsafe { (subslice(self, 0, mid), subslice(self, mid, len)) }
    }

    /// The slice split at `mid`, mutably: `[0, mid)` and `[mid, len)`;
    /// panics if `mid > len`.
    #[track_caller]
    pub fn split_at_mut(&mut self, mid: usize) -> (&mut [T], &mut [T]) {
        let len = self.len();
        if mid > len {
            panic("mid > len")
        }
        unsafe { (subslice_mut(self, 0, mid), subslice_mut(self, mid, len)) }
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
        let mut i = 0;
        while i < src.len() {
            self[i] = src[i];
            i += 1;
        }
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
    pub fn chunks_exact_mut(&mut self, chunk_size: usize) -> ChunksExactMut<'_, T> {
        if chunk_size == 0 {
            panic(c"chunk size must be non-zero")
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
    pub fn rchunks(&self, chunk_size: usize) -> RChunks<'_, T> {
        if chunk_size == 0 {
            panic(c"chunk size must be non-zero")
        }
        RChunks {
            v: self,
            chunk_size,
        }
    }

    /// An iterator over chunks of exactly `chunk_size` elements from the
    /// end, without the remainder at the start; panics if `chunk_size` is
    /// 0.
    pub fn rchunks_exact(&self, chunk_size: usize) -> RChunksExact<'_, T> {
        if chunk_size == 0 {
            panic(c"chunk size must be non-zero")
        }
        let rem = self.len() % chunk_size;
        let (fst, snd) = self.split_at(rem);
        RChunksExact {
            v: snd,
            rem: fst,
            chunk_size,
        }
    }

    /// Copies the elements of `src`, a range of indices, to the ones from
    /// `dest` on, which may overlap them; panics if either is out of
    /// bounds.
    pub fn copy_within<R: RangeBounds<usize>>(&mut self, src: R, dest: usize)
    where
        T: Copy,
    {
        let len = self.len();
        let r = bounds_range(src, len);
        let count = r.end - r.start;
        if dest > len - count {
            panic(c"dest is out of bounds")
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
    pub fn binary_search_by<F: FnMut(&T) -> Ordering>(&self, mut f: F) -> Result<usize, usize> {
        binary_search(self, &mut f)
    }

    /// [`Self::binary_search`] on the keys `f` gives.
    pub fn binary_search_by_key<B: Ord, F: FnMut(&T) -> B>(
        &self,
        b: &B,
        mut f: F,
    ) -> Result<usize, usize> {
        binary_search(self, &mut |k: &T| f(k).cmp(b))
    }

    /// Whether the elements are in order.
    pub fn is_sorted(&self) -> bool
    where
        T: PartialOrd,
    {
        let mut i = 1;
        while i < self.len() {
            if self[i - 1] > self[i] {
                return false;
            }
            i += 1;
        }
        true
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

    /// A mutable pointer to the first element.
    pub const fn as_mut_ptr(&mut self) -> *mut T {
        self as *mut [T] as *mut T
    }
}

/// An iterator over a slice's elements by mutable reference: what
/// [`[T]::iter_mut`] and `for x in &mut slice` give.
#[lang = "slice_iter_mut"]
pub struct IterMut<'a, T> {
    slice: &'a mut [T],
    start: usize,
    end: usize,
}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<&'a mut T> {
        if self.start < self.end {
            let i = self.start;
            self.start += 1;
            // Elements given are not given again, so their borrows may
            // outlive this one of the iterator.
            let rest = unsafe { subslice_mut(self.slice, i, i + 1) };
            Some(&mut rest[0])
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

impl<'a, T> DoubleEndedIterator for IterMut<'a, T> {
    fn next_back(&mut self) -> Option<&'a mut T> {
        if self.start < self.end {
            self.end -= 1;
            let i = self.end;
            let rest = unsafe { subslice_mut(self.slice, i, i + 1) };
            Some(&mut rest[0])
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

impl<T> Index<Range<usize>> for [T] {
    type Output = [T];

    #[track_caller]
    fn index(&self, index: Range<usize>) -> &[T] {
        let r = checked_range(index.start, index.end, self.len());
        unsafe { subslice(self, r.start, r.end) }
    }
}

impl<T> IndexMut<Range<usize>> for [T] {
    #[track_caller]
    fn index_mut(&mut self, index: Range<usize>) -> &mut [T] {
        let r = checked_range(index.start, index.end, self.len());
        unsafe { subslice_mut(self, r.start, r.end) }
    }
}

impl<T> Index<RangeTo<usize>> for [T] {
    type Output = [T];

    #[track_caller]
    fn index(&self, index: RangeTo<usize>) -> &[T] {
        let r = checked_range(0, index.end, self.len());
        unsafe { subslice(self, r.start, r.end) }
    }
}

impl<T> IndexMut<RangeTo<usize>> for [T] {
    #[track_caller]
    fn index_mut(&mut self, index: RangeTo<usize>) -> &mut [T] {
        let r = checked_range(0, index.end, self.len());
        unsafe { subslice_mut(self, r.start, r.end) }
    }
}

impl<T> Index<RangeFrom<usize>> for [T] {
    type Output = [T];

    #[track_caller]
    fn index(&self, index: RangeFrom<usize>) -> &[T] {
        let len = self.len();
        if index.start > len {
            slice_index_fail(index.start, len, len)
        }
        unsafe { subslice(self, index.start, len) }
    }
}

impl<T> IndexMut<RangeFrom<usize>> for [T] {
    #[track_caller]
    fn index_mut(&mut self, index: RangeFrom<usize>) -> &mut [T] {
        let len = self.len();
        if index.start > len {
            slice_index_fail(index.start, len, len)
        }
        unsafe { subslice_mut(self, index.start, len) }
    }
}

impl<T> Index<RangeFull> for [T] {
    type Output = [T];

    #[track_caller]
    fn index(&self, _index: RangeFull) -> &[T] {
        self
    }
}

impl<T> IndexMut<RangeFull> for [T] {
    #[track_caller]
    fn index_mut(&mut self, _index: RangeFull) -> &mut [T] {
        self
    }
}

impl<T> Index<RangeInclusive<usize>> for [T] {
    type Output = [T];

    #[track_caller]
    fn index(&self, index: RangeInclusive<usize>) -> &[T] {
        let end = inclusive_end(index.end);
        let start = if index.exhausted { end } else { index.start };
        let r = checked_range(start, end, self.len());
        unsafe { subslice(self, r.start, r.end) }
    }
}

impl<T> IndexMut<RangeInclusive<usize>> for [T] {
    #[track_caller]
    fn index_mut(&mut self, index: RangeInclusive<usize>) -> &mut [T] {
        let end = inclusive_end(index.end);
        let start = if index.exhausted { end } else { index.start };
        let r = checked_range(start, end, self.len());
        unsafe { subslice_mut(self, r.start, r.end) }
    }
}

impl<T> Index<RangeToInclusive<usize>> for [T] {
    type Output = [T];

    #[track_caller]
    fn index(&self, index: RangeToInclusive<usize>) -> &[T] {
        let r = checked_range(0, inclusive_end(index.end), self.len());
        unsafe { subslice(self, r.start, r.end) }
    }
}

impl<T> IndexMut<RangeToInclusive<usize>> for [T] {
    #[track_caller]
    fn index_mut(&mut self, index: RangeToInclusive<usize>) -> &mut [T] {
        let r = checked_range(0, inclusive_end(index.end), self.len());
        unsafe { subslice_mut(self, r.start, r.end) }
    }
}

/// What `get` and `get_mut` of a slice of `E`s take: a `usize`, for an
/// element, or a range of them, for a part. rustc's `SliceIndex<[E]>`.
#[diagnostic::on_unimplemented(message = "the type `[{E}]` cannot be indexed by `{Self}`")]
pub trait SliceIndex<E> {
    /// An element, or a slice.
    type Output;
    /// What it indexes, if it is within the slice.
    fn get(self, slice: &[E]) -> Option<&Self::Output>;
    /// What it indexes, mutably, if it is within the slice.
    fn get_mut(self, slice: &mut [E]) -> Option<&mut Self::Output>;
}

impl<E> SliceIndex<E> for usize {
    type Output = E;

    fn get(self, slice: &[E]) -> Option<&E> {
        if self < slice.len() { Some(&slice[self]) } else { None }
    }

    fn get_mut(self, slice: &mut [E]) -> Option<&mut E> {
        if self < slice.len() { Some(&mut slice[self]) } else { None }
    }
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

impl<E> SliceIndex<E> for Range<usize> {
    type Output = [E];

    fn get(self, slice: &[E]) -> Option<&[E]> {
        get_range(slice, self.start, self.end)
    }

    fn get_mut(self, slice: &mut [E]) -> Option<&mut [E]> {
        get_range_mut(slice, self.start, self.end)
    }
}

impl<E> SliceIndex<E> for RangeTo<usize> {
    type Output = [E];

    fn get(self, slice: &[E]) -> Option<&[E]> {
        get_range(slice, 0, self.end)
    }

    fn get_mut(self, slice: &mut [E]) -> Option<&mut [E]> {
        get_range_mut(slice, 0, self.end)
    }
}

impl<E> SliceIndex<E> for RangeFrom<usize> {
    type Output = [E];

    fn get(self, slice: &[E]) -> Option<&[E]> {
        let len = slice.len();
        get_range(slice, self.start, len)
    }

    fn get_mut(self, slice: &mut [E]) -> Option<&mut [E]> {
        let len = slice.len();
        get_range_mut(slice, self.start, len)
    }
}

impl<E> SliceIndex<E> for RangeFull {
    type Output = [E];

    fn get(self, slice: &[E]) -> Option<&[E]> {
        Some(slice)
    }

    fn get_mut(self, slice: &mut [E]) -> Option<&mut [E]> {
        Some(slice)
    }
}

impl<E> SliceIndex<E> for RangeInclusive<usize> {
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
}

impl<E> SliceIndex<E> for RangeToInclusive<usize> {
    type Output = [E];

    fn get(self, slice: &[E]) -> Option<&[E]> {
        if self.end == usize::MAX { None } else { get_range(slice, 0, self.end + 1) }
    }

    fn get_mut(self, slice: &mut [E]) -> Option<&mut [E]> {
        if self.end == usize::MAX { None } else { get_range_mut(slice, 0, self.end + 1) }
    }
}

/// The bounds `range` gives for a slice of length `len`, which panics
/// as rustc's `slice::range` does where they are not within it.
fn bounds_range<R: RangeBounds<usize>>(range: R, len: usize) -> Range<usize> {
    let start = match range.start_bound() {
        Bound::Included(&start) => start,
        Bound::Excluded(&start) => {
            if start == usize::MAX {
                panic(c"attempted to index slice from after maximum usize")
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

/// [`[T]::chunks`]'s iterator.
pub struct Chunks<'a, T> {
    v: &'a [T],
    chunk_size: usize,
}

impl<'a, T> Iterator for Chunks<'a, T> {
    type Item = &'a [T];

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
fn binary_search<T, F: FnMut(&T) -> Ordering>(v: &[T], f: &mut F) -> Result<usize, usize> {
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
