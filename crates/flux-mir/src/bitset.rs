//! A set of small indices, a bit each: the states of the dataflow
//! analyses.

use core::ops::Index;

/// A set of the indices below its size, as words of bits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BitSet {
    words: Vec<u64>,
}

impl BitSet {
    /// The empty set of indices below `size`.
    pub(crate) fn new_empty(size: usize) -> BitSet {
        BitSet {
            words: vec![0; size.div_ceil(64)],
        }
    }

    /// The set of every index below `size`.
    pub(crate) fn new_filled(size: usize) -> BitSet {
        let mut words = vec![u64::MAX; size.div_ceil(64)];
        if let Some(last) = words.last_mut()
            && !size.is_multiple_of(64)
        {
            *last = (1 << (size % 64)) - 1;
        }
        BitSet { words }
    }

    pub(crate) fn contains(&self, index: usize) -> bool {
        self.words[index / 64] & (1 << (index % 64)) != 0
    }

    pub(crate) fn insert(&mut self, index: usize) {
        self.words[index / 64] |= 1 << (index % 64);
    }

    pub(crate) fn remove(&mut self, index: usize) {
        self.words[index / 64] &= !(1 << (index % 64));
    }

    /// Puts `index` in the set or takes it out; whether it was in.
    pub(crate) fn set(&mut self, index: usize, value: bool) -> bool {
        let was = self.contains(index);
        if value {
            self.insert(index);
        } else {
            self.remove(index);
        }
        was
    }

    /// Adds the indices of `other`, of the same size; whether any were
    /// not in `self`.
    pub(crate) fn union(&mut self, other: &BitSet) -> bool {
        let mut changed = false;
        for (word, &theirs) in self.words.iter_mut().zip(&other.words) {
            let new = *word | theirs;
            changed |= new != *word;
            *word = new;
        }
        changed
    }
}

/// `set[index]`: whether `index` is in `set`.
impl Index<usize> for BitSet {
    type Output = bool;

    fn index(&self, index: usize) -> &bool {
        if self.contains(index) { &true } else { &false }
    }
}
