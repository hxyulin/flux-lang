//! A max-heap with contiguous storage and logarithmic insertion/removal.
use crate::vec::{self, Vec};
use core::iter::FusedIterator;
use core::slice;

pub struct BinaryHeap<T> {
    data: Vec<T>,
}
impl<T> BinaryHeap<T> {
    pub const fn new() -> Self {
        Self { data: Vec::new() }
    }
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
        }
    }
    pub fn len(&self) -> usize {
        self.data.len()
    }
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    pub fn capacity(&self) -> usize {
        self.data.capacity()
    }
    pub fn reserve(&mut self, additional: usize) {
        self.data.reserve(additional);
    }
    pub fn reserve_exact(&mut self, additional: usize) {
        self.data.reserve_exact(additional);
    }
    pub fn shrink_to_fit(&mut self) {
        self.data.shrink_to_fit();
    }
    pub fn peek(&self) -> Option<&T> {
        self.data.first()
    }
    pub fn as_slice(&self) -> &[T] {
        &self.data
    }
    pub fn clear(&mut self) {
        self.data.clear();
    }
    pub fn into_vec(self) -> Vec<T> {
        self.data
    }
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            inner: self.data.iter(),
        }
    }
}
impl<T: Ord> BinaryHeap<T> {
    fn sift_down(&mut self, mut root: usize, end: usize) {
        while root < end / 2 {
            let left = root * 2 + 1;
            let right = left + 1;
            let child = if right < end && self.data[left] <= self.data[right] {
                right
            } else {
                left
            };
            if self.data[root] >= self.data[child] {
                break;
            }
            self.data.swap(root, child);
            root = child;
        }
    }
    fn rebuild(&mut self) {
        let mut root = self.len() / 2;
        while root != 0 {
            root -= 1;
            self.sift_down(root, self.len());
        }
    }
    pub fn push(&mut self, value: T) {
        self.data.push(value);
        let mut child = self.len() - 1;
        while child != 0 {
            let parent = (child - 1) / 2;
            if self.data[parent] >= self.data[child] {
                break;
            }
            self.data.swap(parent, child);
            child = parent;
        }
    }
    pub fn pop(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        let result = self.data.swap_remove(0);
        self.sift_down(0, self.len());
        Some(result)
    }
    pub fn append(&mut self, other: &mut Self) {
        self.data.append(&mut other.data);
        self.rebuild();
    }
    pub fn into_sorted_vec(mut self) -> Vec<T> {
        let mut end = self.len();
        while end > 1 {
            end -= 1;
            self.data.swap(0, end);
            self.sift_down(0, end);
        }
        self.data
    }
}
impl<T> Default for BinaryHeap<T> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T: Clone> Clone for BinaryHeap<T> {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
        }
    }
}
impl<T: core::fmt::Debug> core::fmt::Debug for BinaryHeap<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}
impl<T: Ord> From<Vec<T>> for BinaryHeap<T> {
    fn from(data: Vec<T>) -> Self {
        let mut result = Self { data };
        result.rebuild();
        result
    }
}
impl<T: Ord, const N: usize> From<[T; N]> for BinaryHeap<T> {
    fn from(data: [T; N]) -> Self {
        Self::from(Vec::from(data))
    }
}
impl<T: Ord> FromIterator<T> for BinaryHeap<T> {
    fn from_iter<I: IntoIterator<Item = T>>(values: I) -> Self {
        Self::from(values.into_iter().collect::<Vec<T>>())
    }
}
impl<T: Ord> Extend<T> for BinaryHeap<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, values: I) {
        for value in values {
            self.push(value);
        }
    }
}
pub struct Iter<'a, T> {
    inner: slice::Iter<'a, T>,
}
impl<'a, T> Clone for Iter<'a, T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<&'a T> {
        self.inner.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}
impl<'a, T> DoubleEndedIterator for Iter<'a, T> {
    fn next_back(&mut self) -> Option<&'a T> {
        self.inner.next_back()
    }
}
impl<T> ExactSizeIterator for Iter<'_, T> {
    fn len(&self) -> usize {
        self.inner.len()
    }
}
impl<T> FusedIterator for Iter<'_, T> {}
pub struct IntoIter<T> {
    inner: vec::IntoIter<T>,
}
impl<T> Iterator for IntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.inner.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}
impl<T> DoubleEndedIterator for IntoIter<T> {
    fn next_back(&mut self) -> Option<T> {
        self.inner.next_back()
    }
}
impl<T> ExactSizeIterator for IntoIter<T> {
    fn len(&self) -> usize {
        self.inner.len()
    }
}
impl<T> FusedIterator for IntoIter<T> {}
impl<T> IntoIterator for BinaryHeap<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;
    fn into_iter(self) -> IntoIter<T> {
        IntoIter {
            inner: self.data.into_iter(),
        }
    }
}
impl<'a, T> IntoIterator for &'a BinaryHeap<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;
    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}
