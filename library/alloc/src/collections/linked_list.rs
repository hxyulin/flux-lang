//! An owning doubly linked list with constant-time endpoint operations.
use crate::boxed::Box;
use core::iter::FusedIterator;
use core::marker::PhantomData;
use core::ptr;

struct Node<T> {
    element: T,
    next: *mut Node<T>,
    prev: *mut Node<T>,
}
pub struct LinkedList<T> {
    head: *mut Node<T>,
    tail: *mut Node<T>,
    len: usize,
    marker: PhantomData<Box<Node<T>>>,
}
// SAFETY: nodes are uniquely owned and shared access exposes only shared elements.
unsafe impl<T: Send> Send for LinkedList<T> {}
// SAFETY: linked storage does not mutate through shared references.
unsafe impl<T: Sync> Sync for LinkedList<T> {}
impl<T> LinkedList<T> {
    pub const fn new() -> Self {
        Self {
            head: ptr::null_mut(),
            tail: ptr::null_mut(),
            len: 0,
            marker: PhantomData,
        }
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn push_front(&mut self, element: T) {
        let len = self.len.checked_add(1).expect("capacity overflow");
        let node = Box::into_raw(Box::new(Node {
            element,
            next: self.head,
            prev: ptr::null_mut(),
        }));
        // SAFETY: nonnull endpoint pointers refer to live, uniquely owned nodes.
        unsafe {
            if self.head.is_null() {
                self.tail = node;
            } else {
                (*self.head).prev = node;
            }
        }
        self.head = node;
        self.len = len;
    }
    pub fn push_back(&mut self, element: T) {
        let len = self.len.checked_add(1).expect("capacity overflow");
        let node = Box::into_raw(Box::new(Node {
            element,
            next: ptr::null_mut(),
            prev: self.tail,
        }));
        // SAFETY: nonnull endpoint pointers refer to live, uniquely owned nodes.
        unsafe {
            if self.tail.is_null() {
                self.head = node;
            } else {
                (*self.tail).next = node;
            }
        }
        self.tail = node;
        self.len = len;
    }
    pub fn pop_front(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        let node = self.head;
        // SAFETY: node is live and owned. Unlink it before transferring its
        // element; rebuilding its Box releases exactly that allocation.
        unsafe {
            self.head = (*node).next;
            self.len -= 1;
            if self.head.is_null() {
                self.tail = ptr::null_mut();
            } else {
                (*self.head).prev = ptr::null_mut();
            }
            let boxed = Box::from_raw(node);
            Some(boxed.element)
        }
    }
    pub fn pop_back(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        let node = self.tail;
        // SAFETY: this is the symmetric unlink/ownership transfer to pop_front.
        unsafe {
            self.tail = (*node).prev;
            self.len -= 1;
            if self.tail.is_null() {
                self.head = ptr::null_mut();
            } else {
                (*self.tail).next = ptr::null_mut();
            }
            let boxed = Box::from_raw(node);
            Some(boxed.element)
        }
    }
    pub fn front(&self) -> Option<&T> {
        // SAFETY: a nonempty list owns a live head for this shared borrow.
        if self.is_empty() {
            None
        } else {
            unsafe { Some(&(*self.head).element) }
        }
    }
    pub fn back(&self) -> Option<&T> {
        // SAFETY: a nonempty list owns a live tail for this shared borrow.
        if self.is_empty() {
            None
        } else {
            unsafe { Some(&(*self.tail).element) }
        }
    }
    pub fn front_mut(&mut self) -> Option<&mut T> {
        // SAFETY: exclusive list access includes the nonempty head element.
        if self.is_empty() {
            None
        } else {
            unsafe { Some(&mut (*self.head).element) }
        }
    }
    pub fn back_mut(&mut self) -> Option<&mut T> {
        // SAFETY: exclusive list access includes the nonempty tail element.
        if self.is_empty() {
            None
        } else {
            unsafe { Some(&mut (*self.tail).element) }
        }
    }
    pub fn clear(&mut self) {
        while let Some(value) = self.pop_front() {
            drop(value);
        }
    }
    pub fn append(&mut self, other: &mut Self) {
        if other.is_empty() {
            return;
        }
        let len = self.len.checked_add(other.len).expect("capacity overflow");
        // SAFETY: the lists own disjoint node allocations and exclusive access
        // permits reconnecting endpoints. Other relinquishes all ownership.
        unsafe {
            if self.is_empty() {
                self.head = other.head;
            } else {
                (*self.tail).next = other.head;
                (*other.head).prev = self.tail;
            }
        }
        self.tail = other.tail;
        self.len = len;
        other.head = ptr::null_mut();
        other.tail = ptr::null_mut();
        other.len = 0;
    }
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            head: self.head,
            tail: self.tail,
            len: self.len,
            marker: PhantomData,
        }
    }
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut {
            head: self.head,
            tail: self.tail,
            len: self.len,
            marker: PhantomData,
        }
    }
}
struct DropGuard<'a, T>(&'a mut LinkedList<T>);
impl<T> Drop for DropGuard<'_, T> {
    fn drop(&mut self) {
        self.0.clear();
    }
}
// SAFETY: payloads are accessed only for their own drop glue; PhantomData<Box<Node<T>>>
// retains all destructor obligations for T. Link maintenance touches no payload.
unsafe impl<#[may_dangle] T> Drop for LinkedList<T> {
    fn drop(&mut self) {
        let guard = DropGuard(self);
        while let Some(value) = guard.0.pop_front() {
            drop(value);
        }
    }
}
impl<T> Default for LinkedList<T> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T: Clone> Clone for LinkedList<T> {
    fn clone(&self) -> Self {
        self.iter().cloned().collect()
    }
}
impl<T: core::fmt::Debug> core::fmt::Debug for LinkedList<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}
impl<T> Extend<T> for LinkedList<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, values: I) {
        for value in values {
            self.push_back(value);
        }
    }
}
impl<T> FromIterator<T> for LinkedList<T> {
    fn from_iter<I: IntoIterator<Item = T>>(values: I) -> Self {
        let mut result = Self::new();
        result.extend(values);
        result
    }
}
impl<T, const N: usize> From<[T; N]> for LinkedList<T> {
    fn from(values: [T; N]) -> Self {
        values.into_iter().collect()
    }
}
pub struct IntoIter<T> {
    list: LinkedList<T>,
}
impl<T> Iterator for IntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.list.pop_front()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.list.len, Some(self.list.len))
    }
}
impl<T> DoubleEndedIterator for IntoIter<T> {
    fn next_back(&mut self) -> Option<T> {
        self.list.pop_back()
    }
}
impl<T> ExactSizeIterator for IntoIter<T> {
    fn len(&self) -> usize {
        self.list.len
    }
}
impl<T> FusedIterator for IntoIter<T> {}
impl<T> IntoIterator for LinkedList<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;
    fn into_iter(self) -> IntoIter<T> {
        IntoIter { list: self }
    }
}
pub struct Iter<'a, T> {
    head: *mut Node<T>,
    tail: *mut Node<T>,
    len: usize,
    marker: PhantomData<&'a T>,
}
impl<'a, T> Clone for Iter<'a, T> {
    fn clone(&self) -> Self {
        Self {
            head: self.head,
            tail: self.tail,
            len: self.len,
            marker: PhantomData,
        }
    }
}
impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<&'a T> {
        if self.len == 0 {
            return None;
        }
        let node = self.head;
        // SAFETY: shared list ownership keeps nodes alive; len bounds both ends.
        unsafe {
            self.head = (*node).next;
            self.len -= 1;
            Some(&(*node).element)
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}
impl<'a, T> DoubleEndedIterator for Iter<'a, T> {
    fn next_back(&mut self) -> Option<&'a T> {
        if self.len == 0 {
            return None;
        }
        let node = self.tail;
        // SAFETY: shared list ownership keeps nodes alive; len bounds both ends.
        unsafe {
            self.tail = (*node).prev;
            self.len -= 1;
            Some(&(*node).element)
        }
    }
}
impl<T> ExactSizeIterator for Iter<'_, T> {
    fn len(&self) -> usize {
        self.len
    }
}
impl<T> FusedIterator for Iter<'_, T> {}
impl<'a, T> IntoIterator for &'a LinkedList<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;
    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}
pub struct IterMut<'a, T> {
    head: *mut Node<T>,
    tail: *mut Node<T>,
    len: usize,
    marker: PhantomData<&'a mut T>,
}
impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;
    fn next(&mut self) -> Option<&'a mut T> {
        if self.len == 0 {
            return None;
        }
        let node = self.head;
        // SAFETY: the exclusive list borrow prevents structural mutation. Each
        // call removes one node from the unyielded interval before borrowing it.
        unsafe {
            self.head = (*node).next;
            self.len -= 1;
            Some(&mut (*node).element)
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}
impl<'a, T> DoubleEndedIterator for IterMut<'a, T> {
    fn next_back(&mut self) -> Option<&'a mut T> {
        if self.len == 0 {
            return None;
        }
        let node = self.tail;
        // SAFETY: the shrinking interval never yields a node twice, including
        // when front and back calls are interleaved.
        unsafe {
            self.tail = (*node).prev;
            self.len -= 1;
            Some(&mut (*node).element)
        }
    }
}
impl<T> ExactSizeIterator for IterMut<'_, T> {
    fn len(&self) -> usize {
        self.len
    }
}
impl<T> FusedIterator for IterMut<'_, T> {}
impl<'a, T> IntoIterator for &'a mut LinkedList<T> {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;
    fn into_iter(self) -> IterMut<'a, T> {
        self.iter_mut()
    }
}

// SAFETY: shared iterators retain a shared list borrow and expose only &T.
unsafe impl<T: Sync> Send for Iter<'_, T> {}
// SAFETY: advancing requires exclusive iterator access; shared access reads only.
unsafe impl<T: Sync> Sync for Iter<'_, T> {}
// SAFETY: mutable iteration owns exclusive access to the unyielded elements.
unsafe impl<T: Send> Send for IterMut<'_, T> {}
// SAFETY: shared iterator access cannot advance or mutate its elements.
unsafe impl<T: Sync> Sync for IterMut<'_, T> {}

impl<'a, T: 'a + Copy> Extend<&'a T> for LinkedList<T> {
    fn extend<I: IntoIterator<Item = &'a T>>(&mut self, iter: I) {
        for &value in iter { self.push_back(value); }
    }
}
