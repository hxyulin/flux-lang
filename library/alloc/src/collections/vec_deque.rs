//! An owning ring buffer with constant-time endpoint insertion and removal.
use crate::raw_vec::{RawVec, min_non_zero_cap};
use core::iter::FusedIterator;
use core::ops::{Index, IndexMut};
use core::{ptr, slice};

/// Elements occupy the `len` slots beginning at `head`, wrapping at capacity.
/// All other slots are uninitialized. RawVec owns only the allocation; this
/// deque alone owns and drops initialized elements.
pub struct VecDeque<T> {
    buf: RawVec<T>,
    head: usize,
    len: usize,
}
// SAFETY: transferring the deque transfers exclusive ownership of every element.
unsafe impl<T: Send> Send for VecDeque<T> {}
// SAFETY: shared access provides shared element references without mutation.
unsafe impl<T: Sync> Sync for VecDeque<T> {}

impl<T> VecDeque<T> {
    pub const fn new() -> Self {
        Self {
            buf: RawVec::new(),
            head: 0,
            len: 0,
        }
    }
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buf: RawVec::with_capacity(capacity),
            head: 0,
            len: 0,
        }
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn capacity(&self) -> usize {
        self.buf.capacity()
    }

    // Unlike (head + offset) % capacity, this cannot overflow for ZST capacity.
    fn index(&self, offset: usize) -> usize {
        let tail = self.capacity() - self.head;
        if offset < tail {
            self.head + offset
        } else {
            offset - tail
        }
    }
    /// Allocates the replacement before transferring any live element.
    pub fn reserve(&mut self, additional: usize) {
        let required = self.len.checked_add(additional).expect("capacity overflow");
        if required <= self.capacity() {
            return;
        }
        let capacity = required
            .max(self.capacity().saturating_mul(2))
            .max(min_non_zero_cap(size_of::<T>()));
        let replacement: RawVec<T> = RawVec::with_capacity(capacity);
        for offset in 0..self.len {
            // SAFETY: each source is initialized and read exactly once. The
            // distinct replacement allocation has capacity for all live items.
            unsafe {
                ptr::write(
                    replacement.ptr().add(offset),
                    ptr::read(self.buf.ptr().add(self.index(offset))),
                );
            }
        }
        self.buf = replacement;
        self.head = 0;
    }
    pub fn push_back(&mut self, value: T) {
        self.reserve(1);
        let index = self.index(self.len);
        // SAFETY: reservation guarantees this uninitialized tail slot exists.
        unsafe {
            ptr::write(self.buf.ptr().add(index), value);
        }
        self.len += 1;
    }
    pub fn push_front(&mut self, value: T) {
        self.reserve(1);
        self.head = if self.head == 0 {
            self.capacity() - 1
        } else {
            self.head - 1
        };
        // SAFETY: after reservation, the slot before the head is uninitialized.
        unsafe {
            ptr::write(self.buf.ptr().add(self.head), value);
        }
        self.len += 1;
    }
    pub fn pop_front(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        let index = self.head;
        self.head = if index == self.capacity() - 1 {
            0
        } else {
            index + 1
        };
        self.len -= 1;
        // SAFETY: the old head was initialized. Ownership is removed before
        // returning the value, so a later destructor cannot drop it again.
        Some(unsafe { ptr::read(self.buf.ptr().add(index)) })
    }
    pub fn pop_back(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        self.len -= 1;
        let index = self.index(self.len);
        // SAFETY: the former final element is initialized and now removed.
        Some(unsafe { ptr::read(self.buf.ptr().add(index)) })
    }
    pub fn get(&self, offset: usize) -> Option<&T> {
        if offset >= self.len {
            return None;
        }
        // SAFETY: this logical position is initialized and the shared borrow
        // prevents mutation or allocation changes for the returned lifetime.
        Some(unsafe { &*self.buf.ptr().add(self.index(offset)) })
    }
    pub fn get_mut(&mut self, offset: usize) -> Option<&mut T> {
        if offset >= self.len {
            return None;
        }
        // SAFETY: this initialized position is exclusively borrowed with self.
        Some(unsafe { &mut *self.buf.ptr().add(self.index(offset)) })
    }
    pub fn front(&self) -> Option<&T> {
        self.get(0)
    }
    pub fn front_mut(&mut self) -> Option<&mut T> {
        self.get_mut(0)
    }
    pub fn back(&self) -> Option<&T> {
        if self.len == 0 {
            None
        } else {
            self.get(self.len - 1)
        }
    }
    pub fn back_mut(&mut self) -> Option<&mut T> {
        if self.len == 0 {
            None
        } else {
            self.get_mut(self.len - 1)
        }
    }
    /// Returns the two initialized contiguous spans, in logical order.
    pub fn as_slices(&self) -> (&[T], &[T]) {
        let first = self.len.min(self.capacity() - self.head);
        // SAFETY: the ring invariant establishes initialized spans head..end
        // and 0..remaining. Empty spans retain RawVec's aligned non-null pointer.
        unsafe {
            (
                slice::from_raw_parts(self.buf.ptr().add(self.head), first),
                slice::from_raw_parts(self.buf.ptr(), self.len - first),
            )
        }
    }
    pub fn as_mut_slices(&mut self) -> (&mut [T], &mut [T]) {
        let first = self.len.min(self.capacity() - self.head);
        // SAFETY: both initialized spans are disjoint, even when one is empty.
        // The exclusive self borrow prevents reallocation or other references.
        unsafe {
            (
                slice::from_raw_parts_mut(self.buf.ptr().add(self.head), first),
                slice::from_raw_parts_mut(self.buf.ptr(), self.len - first),
            )
        }
    }
    pub fn clear(&mut self) {
        let guard = DropGuard(self);
        while let Some(value) = guard.0.pop_front() {
            drop(value);
        }
    }
    pub fn iter(&self) -> Iter<'_, T> {
        let (first, second) = self.as_slices();
        Iter {
            first: first.iter(),
            second: second.iter(),
        }
    }
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        let (first, second) = self.as_mut_slices();
        IterMut {
            first: first.iter_mut(),
            second: second.iter_mut(),
        }
    }
}
struct DropGuard<'a, T>(&'a mut VecDeque<T>);
impl<T> Drop for DropGuard<'_, T> {
    fn drop(&mut self) {
        while let Some(value) = self.0.pop_front() {
            drop(value);
        }
    }
}
// SAFETY: T is accessed only to drop owned initialized elements. RawVec's
// PhantomData<T> retains destructor obligations when T itself needs dropping.
unsafe impl<#[may_dangle] T> Drop for VecDeque<T> {
    fn drop(&mut self) {
        self.clear();
    }
}
impl<T> Default for VecDeque<T> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T: Clone> Clone for VecDeque<T> {
    fn clone(&self) -> Self {
        let mut result = Self::with_capacity(self.len);
        for value in self {
            result.push_back(value.clone());
        }
        result
    }
}
impl<T: core::fmt::Debug> core::fmt::Debug for VecDeque<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}
impl<T> Extend<T> for VecDeque<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, values: I) {
        for value in values {
            self.push_back(value);
        }
    }
}
impl<T> FromIterator<T> for VecDeque<T> {
    fn from_iter<I: IntoIterator<Item = T>>(values: I) -> Self {
        let mut result = Self::new();
        result.extend(values);
        result
    }
}
impl<T, const N: usize> From<[T; N]> for VecDeque<T> {
    fn from(values: [T; N]) -> Self {
        values.into_iter().collect()
    }
}
impl<T> Index<usize> for VecDeque<T> {
    type Output = T;
    #[track_caller]
    fn index(&self, index: usize) -> &T {
        self.get(index).expect("out of bounds access")
    }
}
impl<T> IndexMut<usize> for VecDeque<T> {
    #[track_caller]
    fn index_mut(&mut self, index: usize) -> &mut T {
        self.get_mut(index).expect("out of bounds access")
    }
}

pub struct Iter<'a, T> {
    first: slice::Iter<'a, T>,
    second: slice::Iter<'a, T>,
}
impl<T> Clone for Iter<'_, T> {
    fn clone(&self) -> Self {
        Self {
            first: self.first.clone(),
            second: self.second.clone(),
        }
    }
}
impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<&'a T> {
        self.first.next().or_else(|| self.second.next())
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.first.len() + self.second.len();
        (len, Some(len))
    }
}
impl<'a, T> DoubleEndedIterator for Iter<'a, T> {
    fn next_back(&mut self) -> Option<&'a T> {
        self.second.next_back().or_else(|| self.first.next_back())
    }
}
impl<T> ExactSizeIterator for Iter<'_, T> {
    fn len(&self) -> usize {
        self.first.len() + self.second.len()
    }
}
impl<T> FusedIterator for Iter<'_, T> {}
pub struct IterMut<'a, T> {
    first: slice::IterMut<'a, T>,
    second: slice::IterMut<'a, T>,
}
impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;
    fn next(&mut self) -> Option<&'a mut T> {
        self.first.next().or_else(|| self.second.next())
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.first.len() + self.second.len();
        (len, Some(len))
    }
}
impl<'a, T> DoubleEndedIterator for IterMut<'a, T> {
    fn next_back(&mut self) -> Option<&'a mut T> {
        self.second.next_back().or_else(|| self.first.next_back())
    }
}
impl<T> ExactSizeIterator for IterMut<'_, T> {
    fn len(&self) -> usize {
        self.first.len() + self.second.len()
    }
}
impl<T> FusedIterator for IterMut<'_, T> {}
impl<'a, T> IntoIterator for &'a VecDeque<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;
    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}
impl<'a, T> IntoIterator for &'a mut VecDeque<T> {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;
    fn into_iter(self) -> IterMut<'a, T> {
        self.iter_mut()
    }
}
pub struct IntoIter<T> {
    deque: VecDeque<T>,
}
impl<T> Iterator for IntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.deque.pop_front()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.deque.len, Some(self.deque.len))
    }
}
impl<T> DoubleEndedIterator for IntoIter<T> {
    fn next_back(&mut self) -> Option<T> {
        self.deque.pop_back()
    }
}
impl<T> ExactSizeIterator for IntoIter<T> {
    fn len(&self) -> usize {
        self.deque.len
    }
}
impl<T> FusedIterator for IntoIter<T> {}
impl<T> IntoIterator for VecDeque<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;
    fn into_iter(self) -> IntoIter<T> {
        IntoIter { deque: self }
    }
}

impl<T: core::panic::UnwindSafe> core::panic::UnwindSafe for VecDeque<T> {}
impl<T: core::panic::RefUnwindSafe> core::panic::RefUnwindSafe for VecDeque<T> {}
