//! Slices' methods, and their iterators.

use crate::intrinsics::{subslice, subslice_mut};
use crate::iter::{DoubleEndedIterator, IntoIterator, Iterator};
use crate::ops::{
    Index, IndexMut, Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive,
};
use crate::option::Option::{self, None, Some};
use crate::panicking::panic;

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

    /// The element at `index`, or `None` if it is out of bounds.
    pub fn get(&self, index: usize) -> Option<&T> {
        if index < self.len() { Some(&self[index]) } else { None }
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
}

/// An iterator over a slice's elements by reference: what
/// [`[T]::iter`] and `for x in slice` give.
#[lang = "slice_iter"]
pub struct Iter<'a, T> {
    slice: &'a [T],
    start: usize,
    end: usize,
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
}

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
}

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
fn checked_range(start: usize, end: usize, len: usize) -> Range<usize> {
    if start > end {
        panic(c"slice index starts after it ends")
    }
    if end > len {
        panic(c"range end index out of range for slice")
    }
    Range { start, end }
}

/// `..=end` as `..end + 1`, which panics where `end` is `usize::MAX`.
fn inclusive_end(end: usize) -> usize {
    if end == usize::MAX {
        panic(c"attempted to index slice up to maximum usize")
    }
    end + 1
}

impl<T> Index<Range<usize>> for [T] {
    type Output = [T];

    fn index(&self, index: Range<usize>) -> &[T] {
        let r = checked_range(index.start, index.end, self.len());
        unsafe { subslice(self, r.start, r.end) }
    }
}

impl<T> IndexMut<Range<usize>> for [T] {
    fn index_mut(&mut self, index: Range<usize>) -> &mut [T] {
        let r = checked_range(index.start, index.end, self.len());
        unsafe { subslice_mut(self, r.start, r.end) }
    }
}

impl<T> Index<RangeTo<usize>> for [T] {
    type Output = [T];

    fn index(&self, index: RangeTo<usize>) -> &[T] {
        let r = checked_range(0, index.end, self.len());
        unsafe { subslice(self, r.start, r.end) }
    }
}

impl<T> IndexMut<RangeTo<usize>> for [T] {
    fn index_mut(&mut self, index: RangeTo<usize>) -> &mut [T] {
        let r = checked_range(0, index.end, self.len());
        unsafe { subslice_mut(self, r.start, r.end) }
    }
}

impl<T> Index<RangeFrom<usize>> for [T] {
    type Output = [T];

    fn index(&self, index: RangeFrom<usize>) -> &[T] {
        let len = self.len();
        if index.start > len {
            panic(c"range start index out of range for slice")
        }
        unsafe { subslice(self, index.start, len) }
    }
}

impl<T> IndexMut<RangeFrom<usize>> for [T] {
    fn index_mut(&mut self, index: RangeFrom<usize>) -> &mut [T] {
        let len = self.len();
        if index.start > len {
            panic(c"range start index out of range for slice")
        }
        unsafe { subslice_mut(self, index.start, len) }
    }
}

impl<T> Index<RangeFull> for [T] {
    type Output = [T];

    fn index(&self, _index: RangeFull) -> &[T] {
        self
    }
}

impl<T> IndexMut<RangeFull> for [T] {
    fn index_mut(&mut self, _index: RangeFull) -> &mut [T] {
        self
    }
}

impl<T> Index<RangeInclusive<usize>> for [T] {
    type Output = [T];

    fn index(&self, index: RangeInclusive<usize>) -> &[T] {
        let end = inclusive_end(index.end);
        let start = if index.exhausted { end } else { index.start };
        let r = checked_range(start, end, self.len());
        unsafe { subslice(self, r.start, r.end) }
    }
}

impl<T> IndexMut<RangeInclusive<usize>> for [T] {
    fn index_mut(&mut self, index: RangeInclusive<usize>) -> &mut [T] {
        let end = inclusive_end(index.end);
        let start = if index.exhausted { end } else { index.start };
        let r = checked_range(start, end, self.len());
        unsafe { subslice_mut(self, r.start, r.end) }
    }
}

impl<T> Index<RangeToInclusive<usize>> for [T] {
    type Output = [T];

    fn index(&self, index: RangeToInclusive<usize>) -> &[T] {
        let r = checked_range(0, inclusive_end(index.end), self.len());
        unsafe { subslice(self, r.start, r.end) }
    }
}

impl<T> IndexMut<RangeToInclusive<usize>> for [T] {
    fn index_mut(&mut self, index: RangeToInclusive<usize>) -> &mut [T] {
        let r = checked_range(0, inclusive_end(index.end), self.len());
        unsafe { subslice_mut(self, r.start, r.end) }
    }
}
