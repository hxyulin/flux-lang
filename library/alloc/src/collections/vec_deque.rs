//! An owning ring buffer with constant-time endpoint insertion and removal.
use crate::raw_vec::{RawVec, min_non_zero_cap};
use core::iter::FusedIterator;
use core::ops::{Index, IndexMut, RangeBounds};
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
    pub fn swap(&mut self,a:usize,b:usize){
        assert!(a<self.len && b<self.len,"index out of bounds");
        // SAFETY: both logical indices identify initialized elements of this
        // exclusively borrowed buffer. swap also permits identical pointers.
        unsafe{ptr::swap(self.buf.ptr().add(self.index(a)),self.buf.ptr().add(self.index(b)));}
    }
    pub fn retain<F:FnMut(&T)->bool>(&mut self,mut keep:F){self.retain_mut(|value|keep(value));}
    pub fn retain_mut<F:FnMut(&mut T)->bool>(&mut self,mut keep:F){
        let original=self.len;self.len=0;
        let mut guard=RetainGuard{deque:self,original,processed:0,deleted:0};
        while guard.processed<original{
            // SAFETY: processed identifies an initialized unvisited slot.
            let current=unsafe{guard.deque.buf.ptr().add(guard.deque.index(guard.processed))};
            // SAFETY: the deque is exclusively borrowed and this reference
            // ends before transferring or destroying its element.
            if !keep(unsafe{&mut *current}){
                guard.processed+=1;guard.deleted+=1;
                // SAFETY: remove ownership before user Drop can unwind.
                unsafe{ptr::drop_in_place(current);}
            }else{
                if guard.deleted!=0{
                    // SAFETY: the earlier logical destination is a hole. Both
                    // slots are aligned and distinct for non-zero-sized T.
                    unsafe{ptr::copy_nonoverlapping(current,guard.deque.buf.ptr().add(guard.deque.index(guard.processed-guard.deleted)),1);}
                }
                guard.processed+=1;
            }
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
    /// Rotates the first `mid` elements to the back without allocating.
    ///
    /// # Panics
    /// Panics when `mid` exceeds the deque's length.
    pub fn rotate_left(&mut self, mid: usize) {
        assert!(mid <= self.len, "rotation count exceeds deque length");
        if mid <= self.len - mid {
            for _ in 0..mid {
                let value = self.pop_front().unwrap();
                self.push_back(value);
            }
        } else {
            for _ in 0..self.len - mid {
                let value = self.pop_back().unwrap();
                self.push_front(value);
            }
        }
    }
    /// Rotates the last `count` elements to the front without allocating.
    ///
    /// # Panics
    /// Panics when `count` exceeds the deque's length.
    pub fn rotate_right(&mut self, count: usize) {
        assert!(count <= self.len, "rotation count exceeds deque length");
        self.rotate_left(self.len - count);
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
    /// Removes a range, yielding its elements from either end.
    /// Unconsumed elements are dropped when the iterator is dropped.
    ///
    /// # Panics
    /// Panics if the range is out of bounds or its start exceeds its end.
    #[track_caller]
    pub fn drain<R: RangeBounds<usize>>(&mut self, range: R) -> Drain<'_, T> {
        let len = self.len;
        let core::ops::Range { start, end } = core::slice::range(range, ..len);
        // Keep only the prefix visible. Forgetting Drain may leak the detached
        // range and tail, but cannot expose moved or uninitialized elements.
        self.len = start;
        Drain {
            deque: self,
            front: start,
            back: end,
            tail_start: end,
            tail_len: len - end,
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

/// An iterator owning a removed range while exclusively borrowing its deque.
pub struct Drain<'a, T> {
    deque: &'a mut VecDeque<T>,
    front: usize,
    back: usize,
    tail_start: usize,
    tail_len: usize,
}

impl<T> Iterator for Drain<'_, T> {
    type Item = T;
    const __TRUSTED_LEN: bool = true;

    fn next(&mut self) -> Option<T> {
        if self.front == self.back {
            return None;
        }
        let index = self.deque.index(self.front);
        self.front += 1;
        // SAFETY: this initialized slot belongs to the detached range. Advancing
        // front transfers ownership exactly once, including for zero-sized T.
        Some(unsafe { ptr::read(self.deque.buf.ptr().add(index)) })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.back - self.front;
        (len, Some(len))
    }
}

impl<T> DoubleEndedIterator for Drain<'_, T> {
    fn next_back(&mut self) -> Option<T> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        let index = self.deque.index(self.back);
        // SAFETY: back was decremented before moving the initialized slot out;
        // it is no longer part of the remaining range or the deque's prefix.
        Some(unsafe { ptr::read(self.deque.buf.ptr().add(index)) })
    }
}
impl<T> ExactSizeIterator for Drain<'_, T> {}
impl<T> FusedIterator for Drain<'_, T> {}

impl<T> Drain<'_, T> {
    fn restore_tail(&mut self) {
        let start = self.deque.len;
        for offset in 0..self.tail_len {
            let from = self.deque.index(self.tail_start + offset);
            let to = self.deque.index(start + offset);
            // SAFETY: forward movement copies the initialized tail into the
            // removed gap. Logical ring indices are distinct below capacity,
            // and each source is read before a later move can overwrite it.
            unsafe {
                let value = ptr::read(self.deque.buf.ptr().add(from));
                ptr::write(self.deque.buf.ptr().add(to), value);
            }
        }
        self.deque.len = start + self.tail_len;
    }
}

// Finish dropping the detached range and restore the tail even if an element's
// destructor unwinds. A second destructor panic during unwinding aborts as usual.
struct DrainGuard<'a, 'b, T>(&'a mut Drain<'b, T>);
impl<T> Drop for DrainGuard<'_, '_, T> {
    fn drop(&mut self) {
        while let Some(value) = self.0.next() {
            drop(value);
        }
        self.0.restore_tail();
    }
}
impl<T> Drop for Drain<'_, T> {
    fn drop(&mut self) {
        let guard = DrainGuard(self);
        while let Some(value) = guard.0.next() {
            drop(value);
        }
    }
}

impl<T> From<crate::vec::Vec<T>> for VecDeque<T> {
    fn from(values: crate::vec::Vec<T>) -> Self {
        let (pointer,len,capacity)=values.into_raw_parts();
        // SAFETY: the vector transferred its allocation and initialized range.
        Self { buf:unsafe{RawVec::from_raw_parts(pointer,capacity)},head:0,len }
    }
}
impl<T> From<VecDeque<T>> for crate::vec::Vec<T> {
    fn from(deque: VecDeque<T>) -> Self {
        let len=deque.len;
        let capacity=deque.capacity();
        let first=len.min(capacity-deque.head);
        // Allocate before suppressing the deque's destructor, so allocation
        // failure leaves its initialized elements owned by the deque.
        let mut temporary=crate::vec::Vec::with_capacity(if first<len { first.min(len-first) } else { 0 });
        let values=core::mem::ManuallyDrop::new(deque);
        let pointer=values.buf.ptr();
        // SAFETY: only initialized spans are moved. ManuallyDrop transfers the
        // allocation to the vector, whose single initialized range is rebuilt.
        unsafe {
            if core::mem::size_of::<T>()==0 {
                return crate::vec::Vec::from_raw_parts(pointer,len,capacity);
            }
            if first==len {
                ptr::copy(pointer.add(values.head),pointer,len);
            } else {
                let second=len-first;
                if first<=second {
                    for index in 0..first { temporary.push(ptr::read(pointer.add(values.head+index))); }
                    ptr::copy(pointer,pointer.add(first),second);
                    for (index,value) in temporary.into_iter().enumerate(){ptr::write(pointer.add(index),value);}
                } else {
                    for index in 0..second { temporary.push(ptr::read(pointer.add(index))); }
                    ptr::copy(pointer.add(values.head),pointer,first);
                    for (index,value) in temporary.into_iter().enumerate(){ptr::write(pointer.add(first+index),value);}
                }
            }
            crate::vec::Vec::from_raw_parts(pointer,len,capacity)
        }
    }
}

impl<'a, T: 'a + Copy> Extend<&'a T> for VecDeque<T> {
    fn extend<I: IntoIterator<Item = &'a T>>(&mut self, iter: I) {
        for &value in iter { self.push_back(value); }
    }
}

struct RetainGuard<'a,T>{deque:&'a mut VecDeque<T>,original:usize,processed:usize,deleted:usize}
impl<T> Drop for RetainGuard<'_,T>{
    fn drop(&mut self){
        if self.deleted!=0{
            for read in self.processed..self.original{
                // SAFETY: unvisited elements still own their slots; each moves
                // once into an earlier hole, including across the ring boundary.
                unsafe{ptr::copy_nonoverlapping(self.deque.buf.ptr().add(self.deque.index(read)),self.deque.buf.ptr().add(self.deque.index(read-self.deleted)),1);}
            }
        }
        self.deque.len=self.original-self.deleted;
    }
}
