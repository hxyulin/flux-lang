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

    /// Concatenates slices of cloned elements, accepting Borrow<[U]> owners.
    pub fn concat<U: Clone>(&self) -> Vec<U> where T: Borrow<[U]> {
        let mut result = Vec::new();
        for value in self { result.extend_from_slice(value.borrow()); }
        result
    }
    /// A `Vec` of clones of the elements, with as much room as they take.
    pub fn to_vec(&self) -> Vec<T>
    where
        T: Clone,
    {
        let mut v = Vec::with_capacity(self.len());
        for x in self.iter() {
            v.push(x.clone());
        }
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
