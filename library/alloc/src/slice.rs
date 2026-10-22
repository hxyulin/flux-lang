//! Slices' methods that allocate: `to_vec`, `into_vec`, `repeat`,
//! string `join`.

use crate::string::String;
use crate::vec::Vec;
use core::borrow::Borrow;

impl<T> [T] {
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
