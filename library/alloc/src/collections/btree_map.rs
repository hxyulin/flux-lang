//! An ordered map with logarithmic lookup and amortized logarithmic insertion.
//!
//! The public subset follows `BTreeMap`, but storage is an AVL tree in a flat
//! vector, not the standard library's private B-tree layout. Links are indices;
//! rotations never move keys or values between slots. Iterators use a stack
//! bounded by tree height. No removal, entry, range, or allocator API is exposed.
use crate::vec::Vec;
use core::borrow::Borrow;
use core::cmp::Ordering;

const NONE: usize = usize::MAX;

struct Node<K, V> {
    entry: Option<(K, V)>,
    left: usize,
    right: usize,
    height: usize,
}

pub struct BTreeMap<K, V> {
    nodes: Vec<Node<K, V>>,
    root: usize,
}

impl<K: Clone, V: Clone> Clone for Node<K, V> {
    fn clone(&self) -> Self {
        Self {
            entry: self.entry.clone(),
            left: self.left,
            right: self.right,
            height: self.height,
        }
    }
}

impl<K: Clone, V: Clone> Clone for BTreeMap<K, V> {
    fn clone(&self) -> Self {
        Self {
            nodes: self.nodes.clone(),
            root: self.root,
        }
    }
}

impl<K, V> Default for BTreeMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> BTreeMap<K, V> {
    pub const fn new() -> Self {
        Self {
            nodes: Vec::new(),
            root: NONE,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn iter(&self) -> Iter<'_, K, V> {
        let mut iter = Iter {
            map: self,
            stack: Vec::new(),
        };
        iter.descend(self.root);
        iter
    }

    fn find<Q: ?Sized>(&self, key: &Q) -> usize
    where
        K: Borrow<Q> + Ord,
        Q: Ord,
    {
        let mut index = self.root;
        while index != NONE {
            let node = &self.nodes[index];
            match key.cmp(node.entry.as_ref().unwrap().0.borrow()) {
                Ordering::Less => index = node.left,
                Ordering::Greater => index = node.right,
                Ordering::Equal => return index,
            }
        }
        NONE
    }

    pub fn get<Q: ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q> + Ord,
        Q: Ord,
    {
        let index = self.find(key);
        if index == NONE {
            None
        } else {
            Some(&self.nodes[index].entry.as_ref().unwrap().1)
        }
    }

    pub fn get_mut<Q: ?Sized>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q> + Ord,
        Q: Ord,
    {
        let index = self.find(key);
        if index == NONE {
            None
        } else {
            Some(&mut self.nodes[index].entry.as_mut().unwrap().1)
        }
    }

    pub fn contains_key<Q: ?Sized>(&self, key: &Q) -> bool
    where
        K: Borrow<Q> + Ord,
        Q: Ord,
    {
        self.find(key) != NONE
    }

    fn height(&self, index: usize) -> usize {
        if index == NONE {
            0
        } else {
            self.nodes[index].height
        }
    }

    fn update_height(&mut self, index: usize) {
        let left = self.height(self.nodes[index].left);
        let right = self.height(self.nodes[index].right);
        self.nodes[index].height = 1 + if left > right { left } else { right };
    }

    fn rotate_left(&mut self, index: usize) -> usize {
        let pivot = self.nodes[index].right;
        self.nodes[index].right = self.nodes[pivot].left;
        self.nodes[pivot].left = index;
        self.update_height(index);
        self.update_height(pivot);
        pivot
    }

    fn rotate_right(&mut self, index: usize) -> usize {
        let pivot = self.nodes[index].left;
        self.nodes[index].left = self.nodes[pivot].right;
        self.nodes[pivot].right = index;
        self.update_height(index);
        self.update_height(pivot);
        pivot
    }

    fn balance(&mut self, index: usize) -> usize {
        self.update_height(index);
        let left = self.nodes[index].left;
        let right = self.nodes[index].right;
        if self.height(left) > self.height(right) + 1 {
            if self.height(self.nodes[left].right) > self.height(self.nodes[left].left) {
                self.nodes[index].left = self.rotate_left(left);
            }
            return self.rotate_right(index);
        }
        if self.height(right) > self.height(left) + 1 {
            if self.height(self.nodes[right].left) > self.height(self.nodes[right].right) {
                self.nodes[index].right = self.rotate_right(right);
            }
            return self.rotate_left(index);
        }
        index
    }
}

impl<K: Ord, V> BTreeMap<K, V> {
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let (root, old) = self.insert_at(self.root, key, value);
        self.root = root;
        old
    }

    fn insert_at(&mut self, index: usize, key: K, value: V) -> (usize, Option<V>) {
        if index == NONE {
            let new = self.nodes.len();
            self.nodes.push(Node {
                entry: Some((key, value)),
                left: NONE,
                right: NONE,
                height: 1,
            });
            return (new, None);
        }
        let order = key.cmp(&self.nodes[index].entry.as_ref().unwrap().0);
        let old;
        match order {
            Ordering::Equal => {
                // Keep the original key, including its identity and owned resources.
                let entry = self.nodes[index].entry.as_mut().unwrap();
                return (index, Some(core::mem::replace(&mut entry.1, value)));
            }
            Ordering::Less => {
                let (child, previous) = self.insert_at(self.nodes[index].left, key, value);
                self.nodes[index].left = child;
                old = previous;
            }
            Ordering::Greater => {
                let (child, previous) = self.insert_at(self.nodes[index].right, key, value);
                self.nodes[index].right = child;
                old = previous;
            }
        }
        (self.balance(index), old)
    }
}

pub struct Iter<'a, K, V> {
    map: &'a BTreeMap<K, V>,
    stack: Vec<usize>,
}

impl<K, V> Iter<'_, K, V> {
    fn descend(&mut self, mut index: usize) {
        while index != NONE {
            self.stack.push(index);
            index = self.map.nodes[index].left;
        }
    }
}

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        let index = self.stack.pop()?;
        let node = &self.map.nodes[index];
        self.descend(node.right);
        let entry = node.entry.as_ref().unwrap();
        Some((&entry.0, &entry.1))
    }
}

/// Unvisited entries remain owned by the vector and are dropped with it.
pub struct IntoIter<K, V> {
    map: BTreeMap<K, V>,
    stack: Vec<usize>,
}

impl<K, V> IntoIter<K, V> {
    fn descend(&mut self, mut index: usize) {
        while index != NONE {
            self.stack.push(index);
            index = self.map.nodes[index].left;
        }
    }
}

impl<K, V> IntoIterator for BTreeMap<K, V> {
    type Item = (K, V);
    type IntoIter = IntoIter<K, V>;
    fn into_iter(self) -> Self::IntoIter {
        let root = self.root;
        let mut iter = IntoIter {
            map: self,
            stack: Vec::new(),
        };
        iter.descend(root);
        iter
    }
}

impl<K, V> Iterator for IntoIter<K, V> {
    type Item = (K, V);
    fn next(&mut self) -> Option<Self::Item> {
        let index = self.stack.pop()?;
        let right = self.map.nodes[index].right;
        self.descend(right);
        self.map.nodes[index].entry.take()
    }
}
