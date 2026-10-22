//! `Vec<T>`: a growable array on the heap. It dereferences to a slice,
//! so every slice method works on it, and grows as rustc's does, so its
//! `capacity()` is rustc's. Dropping it drops its elements in order,
//! then frees its memory.

use core::cmp::Ordering;
use core::fmt;
use core::mem::ManuallyDrop;
use core::ops::{Deref, DerefMut, Index, IndexMut};
use core::ptr;

use crate::raw_vec::RawVec;

pub struct Vec<T> {
    buf: RawVec<T>,
    len: usize,
}

impl<T> Vec<T> {
    /// An empty vector, which allocates nothing until it is pushed to.
    pub fn new() -> Vec<T> {
        Vec {
            buf: RawVec::new(),
            len: 0,
        }
    }

    /// An empty vector with room for exactly `capacity` elements.
    pub fn with_capacity(capacity: usize) -> Vec<T> {
        Vec {
            buf: RawVec::with_capacity(capacity),
            len: 0,
        }
    }

    /// The vector whose `length` elements are at `ptr`, in memory
    /// allocated for `capacity` of them.
    pub unsafe fn from_raw_parts(ptr: *mut T, length: usize, capacity: usize) -> Vec<T> {
        Vec {
            buf: unsafe { RawVec::from_raw_parts(ptr, capacity) },
            len: length,
        }
    }

    /// `vec![a, b, c]`: the array's elements, with room for them alone.
    #[doc(hidden)]
    pub fn __from_array<const N: usize>(array: [T; N]) -> Vec<T> {
        let mut v = Vec::with_capacity(N);
        let array = ManuallyDrop::new(array);
        unsafe {
            let src = &*array as *const [T; N] as *const T;
            ptr::copy_nonoverlapping(src, v.as_mut_ptr(), N);
            v.set_len(N);
        }
        v
    }

    /// How many elements it has room for without allocating.
    pub fn capacity(&self) -> usize {
        self.buf.capacity()
    }

    /// Makes room for at least `additional` more elements.
    pub fn reserve(&mut self, additional: usize) {
        self.buf.reserve(self.len, additional);
    }

    /// Makes room for exactly `additional` more elements.
    pub fn reserve_exact(&mut self, additional: usize) {
        self.buf.reserve_exact(self.len, additional);
    }

    /// Gives back the room past its elements.
    pub fn shrink_to_fit(&mut self) {
        if self.capacity() > self.len {
            self.buf.shrink_to_fit(self.len);
        }
    }

    /// Gives back the room past its elements and `min_capacity`.
    pub fn shrink_to(&mut self, min_capacity: usize) {
        if self.capacity() > min_capacity {
            let cap = if self.len > min_capacity {
                self.len
            } else {
                min_capacity
            };
            self.buf.shrink_to_fit(cap);
        }
    }

    /// Drops the elements from `len` on, in order.
    pub fn truncate(&mut self, len: usize) {
        if len >= self.len {
            return;
        }
        let old = self.len;
        self.len = len;
        unsafe { drop_range(self.as_mut_ptr(), len, old) };
    }

    pub fn as_slice(&self) -> &[T] {
        self
    }

    pub fn as_mut_slice(&mut self) -> &mut [T] {
        self
    }

    pub fn as_ptr(&self) -> *const T {
        self.buf.ptr() as *const T
    }

    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.buf.ptr()
    }

    /// Sets the length, without dropping or initializing anything.
    pub unsafe fn set_len(&mut self, new_len: usize) {
        self.len = new_len;
    }

    /// Removes the element at `index`, putting the last in its place.
    pub fn swap_remove(&mut self, index: usize) -> T {
        let len = self.len;
        if index >= len {
            panic!("swap_remove index (is {index}) should be < len (is {len})");
        }
        unsafe {
            let base = self.as_mut_ptr();
            let value = ptr::read(base.add(index));
            ptr::copy(base.add(len - 1), base.add(index), 1);
            self.set_len(len - 1);
            value
        }
    }

    /// Puts `element` at `index`, moving those after it along.
    #[track_caller]
    pub fn insert(&mut self, index: usize, element: T) {
        let len = self.len;
        if index > len {
            panic!("insertion index (is {index}) should be <= len (is {len})");
        }
        if len == self.buf.capacity() {
            self.buf.grow_one();
        }
        unsafe {
            let p = self.as_mut_ptr().add(index);
            if index < len {
                ptr::copy(p, p.add(1), len - index);
            }
            ptr::write(p, element);
            self.set_len(len + 1);
        }
    }

    /// Removes the element at `index`, moving those after it back.
    #[track_caller]
    pub fn remove(&mut self, index: usize) -> T {
        let len = self.len;
        if index >= len {
            panic!("removal index (is {index}) should be < len (is {len})");
        }
        unsafe {
            let p = self.as_mut_ptr().add(index);
            let value = ptr::read(p);
            ptr::copy(p.add(1), p, len - index - 1);
            self.set_len(len - 1);
            value
        }
    }

    /// Keeps the elements `f` accepts, in order, dropping the others as
    /// it meets them.
    pub fn retain<F: FnMut(&T) -> bool>(&mut self, mut f: F) {
        let len = self.len;
        let mut deleted = 0;
        self.len = 0;
        unsafe {
            let base = self.as_mut_ptr();
            for i in 0..len {
                let cur = base.add(i);
                if !f(&*cur) {
                    ptr::drop_in_place(cur);
                    deleted += 1;
                } else if deleted > 0 {
                    ptr::copy_nonoverlapping(cur, base.add(i - deleted), 1);
                }
            }
        }
        self.len = len - deleted;
    }

    /// Adds `value` at the end, growing if it is full.
    pub fn push(&mut self, value: T) {
        let len = self.len;
        if len == self.buf.capacity() {
            self.buf.grow_one();
        }
        unsafe {
            ptr::write(self.as_mut_ptr().add(len), value);
        }
        self.len = len + 1;
    }

    /// Removes the last element.
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            None
        } else {
            self.len -= 1;
            unsafe { Some(ptr::read(self.as_ptr().add(self.len))) }
        }
    }

    /// Moves `other`'s elements to the end of this one.
    pub fn append(&mut self, other: &mut Vec<T>) {
        let count = other.len;
        self.reserve(count);
        unsafe {
            ptr::copy_nonoverlapping(other.as_ptr(), self.as_mut_ptr().add(self.len), count);
            other.set_len(0);
        }
        self.len += count;
    }

    /// Removes the elements in `range`, given by the iterator in turn;
    /// those it does not give are dropped with it, and the elements
    /// after the range moved back.
    #[track_caller]
    pub fn drain<R: core::ops::RangeBounds<usize>>(&mut self, range: R) -> Drain<'_, T> {
        let len = self.len;
        let core::ops::Range { start, end } = core::slice::range(range, ..len);
        self.len = start;
        Drain {
            vec: self,
            start,
            end,
            tail_start: end,
            tail_len: len - end,
        }
    }

    /// Drops every element, in order, keeping the memory.
    pub fn clear(&mut self) {
        self.truncate(0);
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Splits off the elements from `at` on into a new vector.
    #[track_caller]
    pub fn split_off(&mut self, at: usize) -> Vec<T> {
        let len = self.len;
        if at > len {
            panic!("`at` split index (is {at}) should be <= len (is {len})");
        }
        let other_len = len - at;
        let mut other = Vec::with_capacity(other_len);
        unsafe {
            self.set_len(at);
            ptr::copy_nonoverlapping(self.as_ptr().add(at), other.as_mut_ptr(), other_len);
            other.set_len(other_len);
        }
        other
    }

    /// Removes the last element, if `f` accepts it.
    pub fn pop_if(&mut self, f: impl FnOnce(&mut T) -> bool) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        let last = unsafe { &mut *self.as_mut_ptr().add(self.len - 1) };
        if f(last) { self.pop() } else { None }
    }

    /// Removes consecutive equal elements, keeping the first of each run.
    pub fn dedup(&mut self)
    where
        T: PartialEq,
    {
        let len = self.len;
        if len <= 1 {
            return;
        }
        let mut write = 1;
        unsafe {
            let base = self.as_mut_ptr();
            for read in 1..len {
                if *base.add(read) == *base.add(write - 1) {
                    ptr::drop_in_place(base.add(read));
                } else {
                    if read != write {
                        ptr::copy_nonoverlapping(base.add(read), base.add(write), 1);
                    }
                    write += 1;
                }
            }
        }
        self.len = write;
    }

    /// Grows to `new_len` with clones of `value`, or truncates.
    pub fn resize(&mut self, new_len: usize, value: T)
    where
        T: Clone,
    {
        let len = self.len;
        if new_len > len {
            self.extend_with(new_len - len, value);
        } else {
            self.truncate(new_len);
        }
    }

    /// Adds clones of `other`'s elements.
    pub fn extend_from_slice(&mut self, other: &[T])
    where
        T: Clone,
    {
        self.reserve(other.len());
        for x in other.iter() {
            let len = self.len;
            unsafe {
                ptr::write(self.as_mut_ptr().add(len), x.clone());
            }
            self.len = len + 1;
        }
    }

    /// Adds `n` values: clones of `value`, then `value` itself, as
    /// rustc's `extend_with` does.
    fn extend_with(&mut self, n: usize, value: T)
    where
        T: Clone,
    {
        self.reserve(n);
        unsafe {
            let base = self.as_mut_ptr();
            for _ in 1..n {
                ptr::write(base.add(self.len), value.clone());
                self.len += 1;
            }
            if n > 0 {
                ptr::write(base.add(self.len), value);
                self.len += 1;
            }
        }
    }
}

impl<T> Vec<T> {
    /// Adds the items of an iterator whose `size_hint` is exact, with
    /// room made for all of them first, as rustc's `extend_trusted`.
    fn extend_trusted<I: Iterator<Item = T>>(&mut self, iterator: I) {
        let (_, upper) = iterator.size_hint();
        match upper {
            Some(additional) => self.reserve(additional),
            None => crate::raw_vec::capacity_overflow(),
        }
        for element in iterator {
            let len = self.len;
            unsafe { ptr::write(self.as_mut_ptr().add(len), element) };
            self.len = len + 1;
        }
    }

    /// Adds the items of any iterator, making room as rustc's
    /// `extend_desugared` does: for the rest of its lower bound and one
    /// more, each time it is full.
    fn extend_desugared<I: Iterator<Item = T>>(&mut self, mut iterator: I) {
        while let Some(element) = iterator.next() {
            let len = self.len;
            if len == self.capacity() {
                let (lower, _) = iterator.size_hint();
                self.reserve(lower.saturating_add(1));
            }
            unsafe { ptr::write(self.as_mut_ptr().add(len), element) };
            self.len = len + 1;
        }
    }
}

impl<T> FromIterator<T> for Vec<T> {
    /// rustc's `SpecFromIter`: exactly the room an exact iterator says,
    /// else the first item, then room for the rest of the lower bound
    /// (4 elements at least, 8 of a byte, 1 past 1024 bytes).
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Vec<T> {
        let mut iterator = iter.into_iter();
        if iterator.__trusted_len() {
            let mut v = match iterator.size_hint() {
                (_, Some(upper)) => Vec::with_capacity(upper),
                (_, None) => crate::raw_vec::capacity_overflow(),
            };
            v.extend_trusted(iterator);
            return v;
        }
        let first = match iterator.next() {
            None => return Vec::new(),
            Some(first) => first,
        };
        let (lower, _) = iterator.size_hint();
        let min = crate::raw_vec::min_non_zero_cap(size_of::<T>());
        let wanted = lower.saturating_add(1);
        let mut v = Vec::with_capacity(if min > wanted { min } else { wanted });
        unsafe { ptr::write(v.as_mut_ptr(), first) };
        v.len = 1;
        v.extend_desugared(iterator);
        v
    }
}

impl<T> Extend<T> for Vec<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        let iterator = iter.into_iter();
        if iterator.__trusted_len() {
            self.extend_trusted(iterator);
        } else {
            self.extend_desugared(iterator);
        }
    }
}

impl<'a, T: Copy + 'a> Extend<&'a T> for Vec<T> {
    fn extend<I: IntoIterator<Item = &'a T>>(&mut self, iter: I) {
        self.extend(iter.into_iter().copied());
    }
}

/// Drops the values at `base` from `start` to `end`, in order.
unsafe fn drop_range<T>(base: *mut T, start: usize, end: usize) {
    if !core::mem::needs_drop::<T>() {
        return;
    }
    let mut i = start;
    while i < end {
        unsafe { ptr::drop_in_place(base.add(i)) };
        i += 1;
    }
}

/// `vec![elem; n]`: `n - 1` clones of `elem`, then `elem`, with room
/// for exactly them.
#[doc(hidden)]
pub fn from_elem<T: Clone>(elem: T, n: usize) -> Vec<T> {
    let mut v = Vec::with_capacity(n);
    v.extend_with(n, elem);
    v
}

impl<T> Drop for Vec<T> {
    fn drop(&mut self) {
        unsafe { drop_range(self.as_mut_ptr(), 0, self.len) };
    }
}

impl<T> Deref for Vec<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        unsafe { core::slice::from_raw_parts(self.as_ptr(), self.len) }
    }
}

impl<T> DerefMut for Vec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        unsafe { core::slice::from_raw_parts_mut(self.as_mut_ptr(), self.len) }
    }
}

/// `v[i]`, through `Vec`'s own `Index` as in rustc (which panics where
/// the brackets are), then the slice's.
impl<T> Index<usize> for Vec<T> {
    type Output = T;

    #[track_caller]
    fn index(&self, index: usize) -> &T {
        &self.as_slice()[index]
    }
}

impl<T> IndexMut<usize> for Vec<T> {
    #[track_caller]
    fn index_mut(&mut self, index: usize) -> &mut T {
        &mut self.as_mut_slice()[index]
    }
}

macro_rules! range_index {
    ($($range:ty),*) => {
        $(
            impl<T> Index<$range> for Vec<T> {
                type Output = [T];

                #[track_caller]
                fn index(&self, index: $range) -> &[T] {
                    Index::index(self.as_slice(), index)
                }
            }

            impl<T> IndexMut<$range> for Vec<T> {
                #[track_caller]
                fn index_mut(&mut self, index: $range) -> &mut [T] {
                    IndexMut::index_mut(self.as_mut_slice(), index)
                }
            }
        )*
    };
}

range_index!(
    core::ops::Range<usize>,
    core::ops::RangeTo<usize>,
    core::ops::RangeFrom<usize>,
    core::ops::RangeFull,
    core::ops::RangeInclusive<usize>,
    core::ops::RangeToInclusive<usize>
);

impl<T: Clone> Clone for Vec<T> {
    fn clone(&self) -> Vec<T> {
        self.as_slice().to_vec()
    }
}

impl<T> Default for Vec<T> {
    fn default() -> Vec<T> {
        Vec::new()
    }
}

impl<T: fmt::Debug> fmt::Debug for Vec<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_slice(), f)
    }
}

impl<T: PartialEq<U>, U> PartialEq<Vec<U>> for Vec<T> {
    fn eq(&self, other: &Vec<U>) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<T: PartialEq<U>, U> PartialEq<[U]> for Vec<T> {
    fn eq(&self, other: &[U]) -> bool {
        self.as_slice() == other
    }
}

impl<'a, T: PartialEq<U>, U> PartialEq<&'a [U]> for Vec<T> {
    fn eq(&self, other: &&'a [U]) -> bool {
        self.as_slice() == *other
    }
}

impl<'a, T: PartialEq<U>, U> PartialEq<&'a mut [U]> for Vec<T> {
    fn eq(&self, other: &&'a mut [U]) -> bool {
        self.as_slice() == &**other
    }
}

impl<T: PartialEq<U>, U, const N: usize> PartialEq<[U; N]> for Vec<T> {
    fn eq(&self, other: &[U; N]) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<'a, T: PartialEq<U>, U, const N: usize> PartialEq<&'a [U; N]> for Vec<T> {
    fn eq(&self, other: &&'a [U; N]) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<'a, T: PartialEq<U>, U> PartialEq<Vec<U>> for &'a [T] {
    fn eq(&self, other: &Vec<U>) -> bool {
        *self == other.as_slice()
    }
}

impl<T: PartialEq<U>, U> PartialEq<Vec<U>> for [T] {
    fn eq(&self, other: &Vec<U>) -> bool {
        self == other.as_slice()
    }
}

impl<T: Eq> Eq for Vec<T> {}

impl<T: PartialOrd> PartialOrd for Vec<T> {
    fn partial_cmp(&self, other: &Vec<T>) -> Option<Ordering> {
        // Element by element, as slices compare.
        let (a, b) = (self.as_slice(), other.as_slice());
        let n = if a.len() < b.len() { a.len() } else { b.len() };
        for i in 0..n {
            match a[i].partial_cmp(&b[i]) {
                Some(Ordering::Equal) => {}
                other => return other,
            }
        }
        a.len().partial_cmp(&b.len())
    }
}

impl<T: Ord> Ord for Vec<T> {
    fn cmp(&self, other: &Vec<T>) -> Ordering {
        self.as_slice().cmp(other.as_slice())
    }
}

impl<T: Clone> From<&[T]> for Vec<T> {
    fn from(s: &[T]) -> Vec<T> {
        s.to_vec()
    }
}

impl<T, const N: usize> From<[T; N]> for Vec<T> {
    fn from(array: [T; N]) -> Vec<T> {
        Vec::__from_array(array)
    }
}

impl<T> IntoIterator for Vec<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> IntoIter<T> {
        let me = ManuallyDrop::new(self);
        let buf = unsafe { ptr::read(&me.buf) };
        IntoIter {
            buf,
            start: 0,
            end: me.len,
        }
    }
}

impl<'a, T> IntoIterator for &'a Vec<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;

    fn into_iter(self) -> core::slice::Iter<'a, T> {
        self.as_slice().iter()
    }
}

impl<'a, T> IntoIterator for &'a mut Vec<T> {
    type Item = &'a mut T;
    type IntoIter = core::slice::IterMut<'a, T>;

    fn into_iter(self) -> core::slice::IterMut<'a, T> {
        self.as_mut_slice().iter_mut()
    }
}

/// [`Vec::drain`]'s iterator.
pub struct Drain<'a, T> {
    vec: &'a mut Vec<T>,
    /// The next element it gives, and one past the last.
    start: usize,
    end: usize,
    /// Where the elements after the range are, and how many.
    tail_start: usize,
    tail_len: usize,
}

impl<T> Drain<'_, T> {
    /// The elements not given yet.
    pub fn as_slice(&self) -> &[T] {
        unsafe {
            core::slice::from_raw_parts(self.vec.as_ptr().add(self.start), self.end - self.start)
        }
    }
}

impl<T> Iterator for Drain<'_, T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        if self.start == self.end {
            return None;
        }
        let value = unsafe { ptr::read(self.vec.as_ptr().add(self.start)) };
        self.start += 1;
        Some(value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.end - self.start;
        (n, Some(n))
    }

    #[doc(hidden)]
    fn __trusted_len(&self) -> bool {
        true
    }
}

impl<T> DoubleEndedIterator for Drain<'_, T> {
    fn next_back(&mut self) -> Option<T> {
        if self.start == self.end {
            return None;
        }
        self.end -= 1;
        Some(unsafe { ptr::read(self.vec.as_ptr().add(self.end)) })
    }
}

impl<T> ExactSizeIterator for Drain<'_, T> {}

impl<T> Drop for Drain<'_, T> {
    /// The elements not given, in order, then the tail moved back.
    fn drop(&mut self) {
        unsafe {
            drop_range(self.vec.as_mut_ptr(), self.start, self.end);
            if self.tail_len > 0 {
                let start = self.vec.len;
                if self.tail_start != start {
                    let base = self.vec.as_mut_ptr();
                    ptr::copy(base.add(self.tail_start), base.add(start), self.tail_len);
                }
                self.vec.len = start + self.tail_len;
            }
        }
    }
}

/// A vector's elements, moved out one by one from either end; those
/// not taken are dropped with it, and its memory freed.
pub struct IntoIter<T> {
    buf: RawVec<T>,
    start: usize,
    end: usize,
}

impl<T> IntoIter<T> {
    /// The elements not taken yet.
    pub fn as_slice(&self) -> &[T] {
        unsafe { core::slice::from_raw_parts(self.buf.ptr().add(self.start), self.end - self.start) }
    }
}

impl<T> Iterator for IntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        if self.start == self.end {
            return None;
        }
        let value = unsafe { ptr::read(self.buf.ptr().add(self.start)) };
        self.start += 1;
        Some(value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.end - self.start;
        (n, Some(n))
    }
}

impl<T> DoubleEndedIterator for IntoIter<T> {
    fn next_back(&mut self) -> Option<T> {
        if self.start == self.end {
            return None;
        }
        self.end -= 1;
        Some(unsafe { ptr::read(self.buf.ptr().add(self.end)) })
    }
}

impl<T> ExactSizeIterator for IntoIter<T> {}

impl<T> Drop for IntoIter<T> {
    fn drop(&mut self) {
        unsafe { drop_range(self.buf.ptr(), self.start, self.end) };
    }
}
