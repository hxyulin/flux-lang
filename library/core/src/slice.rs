//! Slices' methods, and their iterators.

use crate::intrinsics::subslice_mut;
use crate::iter::{DoubleEndedIterator, IntoIterator, Iterator};
use crate::option::Option::{self, None, Some};

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
