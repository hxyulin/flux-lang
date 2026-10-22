//! Slices' methods that allocate: `to_vec`, `into_vec`, `repeat`,
//! `concat`.

use crate::vec::Vec;

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
