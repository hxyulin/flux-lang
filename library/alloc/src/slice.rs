//! Slices' methods that allocate: `to_vec`, `into_vec`, `repeat`,
//! string `join`.

use crate::string::String;
use crate::vec::Vec;
use core::borrow::Borrow;

impl<T> [T] {
    /// Sorts stably while evaluating each element's key at most once.
    pub fn sort_by_cached_key<K: Ord, F: FnMut(&T) -> K>(&mut self, mut f: F) {
        let mut indices = Vec::with_capacity(self.len());
        for (index, value) in self.iter().enumerate() { indices.push((f(value), index)); }
        indices.sort_unstable_by(|a: &(K, usize), b: &(K, usize)| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        for i in 0..indices.len() {
            let mut index = indices[i].1;
            while index < i { index = indices[index].1; }
            indices[i].1 = index;
            self.swap(i, index);
        }
    }

    /// Concatenates borrowed slices or strings into their owned representation.
    pub fn concat<Item: ?Sized>(&self) -> <Self as Concat<Item>>::Output
    where Self: Concat<Item> {
        <Self as Concat<Item>>::concat(self)
    }
    /// A `Vec` of clones of the elements, with as much room as they take.
    pub fn to_vec(&self) -> Vec<T>
    where
        T: Clone,
    {
        let mut v = Vec::with_capacity(self.len());
        v.extend_from_slice(self);
        v
    }

    /// The elements, `n` times over.
    pub fn repeat(&self, n: usize) -> Vec<T>
    where
        T: Copy,
    {
        let mut v = Vec::with_capacity(self.len() * n);
        for _ in 0..n {
            v.extend_from_slice(self);
        }
        v
    }
}

impl<T: Borrow<str>> [T] {
    /// Concatenates the borrowed strings, with `separator` between elements.
    pub fn join(&self, separator: &str) -> String {
        let mut result = String::new();
        let mut first = true;
        for value in self {
            if !first {
                result.push_str(separator);
            }
            first = false;
            result.push_str(value.borrow());
        }
        result
    }
}

/// The owned result of concatenating a slice of borrowed values.
#[doc(hidden)]
pub(crate) trait Concat<Item: ?Sized> {
    type Output;
    fn concat(slice: &Self) -> Self::Output;
}
impl<T: Borrow<[U]>, U: Clone> Concat<[U]> for [T] {
    type Output = Vec<U>;
    fn concat(slice: &Self) -> Vec<U> {
        let mut result = Vec::new();
        for value in slice { result.extend_from_slice(value.borrow()); }
        result
    }
}
impl<T: Borrow<str>> Concat<str> for [T] {
    type Output = String;
    fn concat(slice: &Self) -> String {
        let mut result = String::new();
        for value in slice { result.push_str(value.borrow()); }
        result
    }
}
