//! An ordered map with logarithmic lookup and amortized logarithmic insertion.
//!
//! The public subset follows `BTreeMap`, but storage is an AVL tree in a flat
//! vector, not the standard library's private B-tree layout. Links are indices;
//! rotations never move keys or values between slots. Iterators use a stack
//! bounded by tree height. Removed slots are reused through an index free list.
//! Allocator-parameter APIs remain outside this subset.
use crate::vec::Vec;
use core::borrow::Borrow;
use core::cmp::Ordering;
use core::hash::{Hash, Hasher};

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
    free: usize,
    len: usize,
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
            free: self.free,
            len: self.len,
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
            free: NONE,
            len: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn len(&self) -> usize {
        self.len
    }

    /// Removes every entry before dropping its former storage.
    pub fn clear(&mut self) { drop(core::mem::replace(self, Self::new())); }

    pub fn into_values(self) -> IntoValues<K, V> { IntoValues { inner: self.into_iter() } }

    pub fn iter(&self) -> Iter<'_, K, V> {
        let mut iter = Iter {
            map: self,
            stack: Vec::new(),
            back_stack: Vec::new(),
            remaining: self.len,
        };
        iter.descend(self.root);
        iter.descend_back(self.root);
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

    pub fn get_key_value<Q: ?Sized>(&self, key: &Q) -> Option<(&K, &V)>
    where K: Borrow<Q> + Ord, Q: Ord {
        let index = self.find(key);
        if index == NONE { None }
        else { self.nodes[index].entry.as_ref().map(|(key, value)| (key, value)) }
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
    /// Moves the source entries, replacing equal values but retaining our keys.
    pub fn append(&mut self, other: &mut Self) {
        for (key, value) in core::mem::take(other) { self.insert(key, value); }
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let (root, old, _) = self.insert_at(self.root, key, value);
        self.root = root;
        old
    }

    fn insert_at(&mut self, index: usize, key: K, value: V) -> (usize, Option<V>, usize) {
        if index == NONE {
            let node = Node { entry: Some((key, value)), left: NONE, right: NONE, height: 1 };
            let new = if self.free == NONE {
                let new = self.nodes.len();
                self.nodes.push(node);
                new
            } else {
                let new = self.free;
                self.free = self.nodes[new].right;
                self.nodes[new] = node;
                new
            };
            self.len += 1;
            return (new, None, new);
        }
        let order = key.cmp(&self.nodes[index].entry.as_ref().unwrap().0);
        let old;
        let inserted;
        match order {
            Ordering::Equal => {
                // Keep the original key, including its identity and owned resources.
                let entry = self.nodes[index].entry.as_mut().unwrap();
                return (index, Some(core::mem::replace(&mut entry.1, value)), index);
            }
            Ordering::Less => {
                let (child, previous, slot) = self.insert_at(self.nodes[index].left, key, value);
                self.nodes[index].left = child;
                old = previous;
                inserted = slot;
            }
            Ordering::Greater => {
                let (child, previous, slot) = self.insert_at(self.nodes[index].right, key, value);
                self.nodes[index].right = child;
                old = previous;
                inserted = slot;
            }
        }
        (self.balance(index), old, inserted)
    }
}

pub struct Iter<'a, K, V> {
    map: &'a BTreeMap<K, V>,
    stack: Vec<usize>,
    back_stack: Vec<usize>,
    remaining: usize,
}

impl<K, V> Iter<'_, K, V> {
    fn descend_back(&mut self, mut index: usize) {
        while index != NONE {
            self.back_stack.push(index);
            index = self.map.nodes[index].right;
        }
    }
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
        if self.remaining == 0 { return None; }
        let index = self.stack.pop()?;
        let node = &self.map.nodes[index];
        self.remaining -= 1;
        self.descend(node.right);
        let entry = node.entry.as_ref().unwrap();
        Some((&entry.0, &entry.1))
    }
    fn size_hint(&self) -> (usize, Option<usize>) { (self.remaining, Some(self.remaining)) }
}

impl<'a, K, V> core::iter::DoubleEndedIterator for Iter<'a, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 { return None; }
        let index = self.back_stack.pop()?;
        let node = &self.map.nodes[index];
        self.remaining -= 1;
        self.descend_back(node.left);
        node.entry.as_ref().map(|(key, value)| (key, value))
    }
}

/// Unvisited entries remain owned by the vector and are dropped with it.
pub struct IntoIter<K, V> {
    map: BTreeMap<K, V>,
    stack: Vec<usize>,
    back_stack: Vec<usize>,
}

impl<K, V> IntoIter<K, V> {
    fn descend(&mut self, mut index: usize) {
        while index != NONE {
            self.stack.push(index);
            index = self.map.nodes[index].left;
        }
    }
}

impl<K, V> IntoIter<K, V> {
    fn descend_back(&mut self, mut index: usize) {
        while index != NONE {
            self.back_stack.push(index);
            index = self.map.nodes[index].right;
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
            back_stack: Vec::new(),
        };
        iter.descend(root);
        iter.descend_back(root);
        iter
    }
}

impl<K, V> Iterator for IntoIter<K, V> {
    type Item = (K, V);
    fn next(&mut self) -> Option<Self::Item> {
        if self.map.len == 0 { return None; }
        let index = self.stack.pop()?;
        let right = self.map.nodes[index].right;
        self.descend(right);
        self.map.len -= 1;
        self.map.nodes[index].entry.take()
    }
    fn size_hint(&self) -> (usize, Option<usize>) { (self.map.len, Some(self.map.len)) }
}
impl<K, V> core::iter::DoubleEndedIterator for IntoIter<K, V> {
    fn next_back(&mut self) -> Option<(K, V)> {
        if self.map.len == 0 { return None; }
        let index = self.back_stack.pop()?;
        let left = self.map.nodes[index].left;
        self.descend_back(left);
        self.map.len -= 1;
        self.map.nodes[index].entry.take()
    }
}
impl<K, V> core::iter::ExactSizeIterator for IntoIter<K, V> {}
impl<K, V> core::iter::FusedIterator for IntoIter<K, V> {}

pub struct IntoValues<K, V> { inner: IntoIter<K, V> }
impl<K, V> Iterator for IntoValues<K, V> {
    type Item = V;
    fn next(&mut self) -> Option<V> { self.inner.next().map(|(_, value)| value) }
    fn size_hint(&self) -> (usize, Option<usize>) { self.inner.size_hint() }
}
impl<K, V> core::iter::DoubleEndedIterator for IntoValues<K, V> {
    fn next_back(&mut self) -> Option<V> { self.inner.next_back().map(|(_, value)| value) }
}
impl<K, V> core::iter::ExactSizeIterator for IntoValues<K, V> {}
impl<K, V> core::iter::FusedIterator for IntoValues<K, V> {}

impl<K, V> BTreeMap<K, V> {
    fn iteration_order(&self) -> Vec<usize> {
        let mut order = Vec::new();
        let mut stack = Vec::new();
        let mut at = self.root;
        while at != NONE || !stack.is_empty() {
            while at != NONE { stack.push(at); at = self.nodes[at].left; }
            at = stack.pop().unwrap();
            order.push(at);
            at = self.nodes[at].right;
        }
        order
    }

    /// Every occupied slot is borrowed exclusively once. Taking its tuple
    /// prevents either iterator end from yielding the same value twice.
    pub fn iter_mut(&mut self) -> IterMut<'_, K, V> {
        let order = self.iteration_order().into_iter();
        let entries = self.nodes.iter_mut().map(|node| {
            node.entry.as_mut().map(|(key, value)| (&*key, value))
        }).collect();
        IterMut { order, entries, keys: core::marker::PhantomData }
    }
    pub fn values_mut(&mut self) -> ValuesMut<'_, K, V> {
        ValuesMut { inner: self.iter_mut() }
    }
}

pub struct IterMut<'a, K, V> {
    order: crate::vec::IntoIter<usize>,
    entries: Vec<Option<(&'a K, &'a mut V)>>,
    keys: core::marker::PhantomData<&'a mut K>,
}
// SAFETY: each remaining tuple owns an exclusive node borrow from iter_mut.
// Already yielded nodes have been removed, so moving the remaining disjoint
// nodes needs K: Send, not K: Sync, even though their keys are read-only.
unsafe impl<K: Send, V: Send> Send for IterMut<'_, K, V> {}
impl<'a, K, V> Iterator for IterMut<'a, K, V> {
    type Item = (&'a K, &'a mut V);
    fn next(&mut self) -> Option<Self::Item> {
        self.order.next().and_then(|index| self.entries[index].take())
    }
    fn size_hint(&self) -> (usize, Option<usize>) { self.order.size_hint() }
}
impl<'a, K, V> core::iter::DoubleEndedIterator for IterMut<'a, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.order.next_back().and_then(|index| self.entries[index].take())
    }
}
impl<K, V> core::iter::ExactSizeIterator for IterMut<'_, K, V> {}
impl<K, V> core::iter::FusedIterator for IterMut<'_, K, V> {}

pub struct ValuesMut<'a, K, V> { inner: IterMut<'a, K, V> }
impl<'a, K, V> Iterator for ValuesMut<'a, K, V> {
    type Item = &'a mut V;
    fn next(&mut self) -> Option<Self::Item> { self.inner.next().map(|(_, value)| value) }
    fn size_hint(&self) -> (usize, Option<usize>) { self.inner.size_hint() }
}
impl<'a, K, V> core::iter::DoubleEndedIterator for ValuesMut<'a, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> { self.inner.next_back().map(|(_, value)| value) }
}
impl<K, V> core::iter::ExactSizeIterator for ValuesMut<'_, K, V> {}
impl<K, V> core::iter::FusedIterator for ValuesMut<'_, K, V> {}

impl<K, V, Q: ?Sized> core::ops::Index<&Q> for BTreeMap<K, V>
where K: Borrow<Q> + Ord, Q: Ord {
    type Output = V;
    fn index(&self, key: &Q) -> &V { self.get(key).expect("no entry found for key") }
}

impl<K, V> BTreeMap<K, V> {
    pub fn keys(&self) -> Keys<'_, K, V> { Keys { inner: self.iter() } }
    pub fn values(&self) -> Values<'_, K, V> { Values { inner: self.iter() } }

    pub fn first_key_value(&self) -> Option<(&K, &V)> where K: Ord {
        let mut index = self.root;
        if index == NONE { return None; }
        while self.nodes[index].left != NONE { index = self.nodes[index].left; }
        self.nodes[index].entry.as_ref().map(|(key, value)| (key, value))
    }

    pub fn last_key_value(&self) -> Option<(&K, &V)> where K: Ord {
        let mut index = self.root;
        if index == NONE { return None; }
        while self.nodes[index].right != NONE { index = self.nodes[index].right; }
        self.nodes[index].entry.as_ref().map(|(key, value)| (key, value))
    }

    pub fn pop_first(&mut self) -> Option<(K, V)> where K: Ord {
        if self.root == NONE { return None; }
        let (root, first) = self.detach_min(self.root);
        self.root = root;
        let entry = self.nodes[first].entry.take();
        self.release(first);
        entry
    }

    /// Visits entries in ascending key order, retaining those accepted by `f`.
    pub fn retain<F: FnMut(&K, &mut V) -> bool>(&mut self, mut f: F) where K: Ord {
        let mut order = Vec::new();
        let mut stack = Vec::new();
        let mut at = self.root;
        while at != NONE || !stack.is_empty() {
            while at != NONE { stack.push(at); at = self.nodes[at].left; }
            at = stack.pop().unwrap();
            order.push(at);
            at = self.nodes[at].right;
        }
        for index in order {
            let keep = {
                let (key, value) = self.nodes[index].entry.as_mut().unwrap();
                f(key, value)
            };
            if !keep {
                // Determine the path before changing ownership. A comparison
                // panic leaves all links and entries intact.
                let mut path = Vec::new();
                let key = &self.nodes[index].entry.as_ref().unwrap().0;
                let mut at = self.root;
                while at != index {
                    let left = key < &self.nodes[at].entry.as_ref().unwrap().0;
                    path.push(left);
                    at = if left { self.nodes[at].left } else { self.nodes[at].right };
                }
                let (root, removed) = self.remove_path(self.root, &path);
                self.root = root;
                drop(removed);
            }
        }
    }

    // Keep surviving entries in their original slots, including successors
    // already queued by retain. No user comparisons run after unlinking.
    fn remove_path(&mut self, index: usize, path: &[bool]) -> (usize, Option<(K, V)>) {
        if let Some((&left, rest)) = path.split_first() {
            let child = if left { self.nodes[index].left } else { self.nodes[index].right };
            let (child, entry) = self.remove_path(child, rest);
            if left { self.nodes[index].left = child; } else { self.nodes[index].right = child; }
            return (self.balance(index), entry);
        }
        let left = self.nodes[index].left;
        let right = self.nodes[index].right;
        let entry = self.nodes[index].entry.take();
        let root = if left == NONE { right } else if right == NONE { left } else {
            let (right, next) = self.detach_min(right);
            self.nodes[next].left = left;
            self.nodes[next].right = right;
            self.balance(next)
        };
        self.release(index);
        (root, entry)
    }

    /// Iterates over keys in the given bounds without copying entries.
    ///
    /// # Panics
    /// Panics for reversed bounds or two equal excluded bounds.
    pub fn range<Q: ?Sized, R>(&self, range: R) -> Range<'_, K, V>
    where K: Borrow<Q> + Ord, Q: Ord, R: core::ops::RangeBounds<Q> {
        use core::ops::Bound::{Included, Excluded, Unbounded};
        // As in Rust's map, an empty tree has no interval to validate.
        if self.root == NONE {
            return Range { map: self, front: Vec::new(), back: Vec::new(), finished: true };
        }
        let start = range.start_bound();
        let end = range.end_bound();
        match (&start, &end) {
            (Included(a) | Excluded(a), Included(b) | Excluded(b)) => {
                match a.cmp(b) {
                    Ordering::Greater => panic!("range start is greater than range end in BTreeMap"),
                    Ordering::Equal if matches!((&start, &end), (Excluded(_), Excluded(_))) =>
                        panic!("range start and end are equal and excluded in BTreeMap"),
                    _ => {},
                }
            }
            _ => {},
        }
        let mut front = Vec::new();
        let mut at = self.root;
        while at != NONE {
            let key: &Q = self.nodes[at].entry.as_ref().unwrap().0.borrow();
            let accept = match start { Included(bound) => key >= bound, Excluded(bound) => key > bound, Unbounded => true };
            if accept { front.push(at); at = self.nodes[at].left; }
            else { at = self.nodes[at].right; }
        }
        let mut back = Vec::new();
        at = self.root;
        while at != NONE {
            let key: &Q = self.nodes[at].entry.as_ref().unwrap().0.borrow();
            let accept = match end { Included(bound) => key <= bound, Excluded(bound) => key < bound, Unbounded => true };
            if accept { back.push(at); at = self.nodes[at].right; }
            else { at = self.nodes[at].left; }
        }
        let finished = match (front.last(), back.last()) {
            (Some(&a), Some(&b)) => self.nodes[a].entry.as_ref().unwrap().0 > self.nodes[b].entry.as_ref().unwrap().0,
            _ => true,
        };
        Range { map: self, front, back, finished }
    }

    pub fn remove<Q: ?Sized>(&mut self, key: &Q) -> Option<V>
    where K: Borrow<Q> + Ord, Q: Ord {
        self.remove_entry(key).map(|(_, value)| value)
    }

    pub fn remove_entry<Q: ?Sized>(&mut self, key: &Q) -> Option<(K, V)>
    where K: Borrow<Q> + Ord, Q: Ord {
        let (root, removed) = self.remove_at(self.root, key);
        self.root = root;
        removed
    }

    fn remove_at<Q: ?Sized>(&mut self, index: usize, key: &Q) -> (usize, Option<(K, V)>)
    where K: Borrow<Q> + Ord, Q: Ord {
        if index == NONE { return (NONE, None); }
        let order = key.cmp(self.nodes[index].entry.as_ref().unwrap().0.borrow());
        let removed;
        match order {
            Ordering::Less => {
                let (child, old) = self.remove_at(self.nodes[index].left, key);
                if old.is_none() { return (index, None); }
                self.nodes[index].left = child;
                removed = old;
            }
            Ordering::Greater => {
                let (child, old) = self.remove_at(self.nodes[index].right, key);
                if old.is_none() { return (index, None); }
                self.nodes[index].right = child;
                removed = old;
            }
            Ordering::Equal => {
                let left = self.nodes[index].left;
                let right = self.nodes[index].right;
                let entry = self.nodes[index].entry.take();
                if left == NONE || right == NONE {
                    self.release(index);
                    return (if left == NONE { right } else { left }, entry);
                }
                let (child, successor) = self.detach_min(right);
                self.nodes[index].right = child;
                self.nodes[index].entry = self.nodes[successor].entry.take();
                self.release(successor);
                removed = entry;
            }
        }
        (self.balance(index), removed)
    }

    // Detaching a successor never compares or drops keys. Once removal changes
    // ownership, unwinding user comparisons can no longer interrupt rebalancing.
    fn detach_min(&mut self, index: usize) -> (usize, usize) {
        let left = self.nodes[index].left;
        if left == NONE { return (self.nodes[index].right, index); }
        let (child, minimum) = self.detach_min(left);
        self.nodes[index].left = child;
        (self.balance(index), minimum)
    }

    fn release(&mut self, index: usize) {
        self.nodes[index].right = self.free;
        self.free = index;
        self.len -= 1;
    }
}

impl<K, V> Clone for Iter<'_, K, V> {
    fn clone(&self) -> Self {
        Self { map: self.map, stack: self.stack.clone(), back_stack: self.back_stack.clone(), remaining: self.remaining }
    }
}
impl<K, V> core::iter::ExactSizeIterator for Iter<'_, K, V> {}
impl<K, V> core::iter::FusedIterator for Iter<'_, K, V> {}

pub struct Keys<'a, K, V> { inner: Iter<'a, K, V> }
impl<'a, K, V> Iterator for Keys<'a, K, V> {
    type Item = &'a K;
    fn next(&mut self) -> Option<Self::Item> { self.inner.next().map(|(key, _)| key) }
    fn size_hint(&self) -> (usize, Option<usize>) { self.inner.size_hint() }
}
impl<'a, K, V> core::iter::DoubleEndedIterator for Keys<'a, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> { self.inner.next_back().map(|(key, _)| key) }
}
impl<K, V> Clone for Keys<'_, K, V> {
    fn clone(&self) -> Self { Self { inner: self.inner.clone() } }
}
impl<K, V> core::iter::ExactSizeIterator for Keys<'_, K, V> {}
impl<K, V> core::iter::FusedIterator for Keys<'_, K, V> {}

pub struct Values<'a, K, V> { inner: Iter<'a, K, V> }
impl<'a, K, V> Iterator for Values<'a, K, V> {
    type Item = &'a V;
    fn next(&mut self) -> Option<Self::Item> { self.inner.next().map(|(_, value)| value) }
    fn size_hint(&self) -> (usize, Option<usize>) { self.inner.size_hint() }
}
impl<'a, K, V> core::iter::DoubleEndedIterator for Values<'a, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> { self.inner.next_back().map(|(_, value)| value) }
}
impl<K, V> Clone for Values<'_, K, V> {
    fn clone(&self) -> Self { Self { inner: self.inner.clone() } }
}
impl<K, V> core::iter::ExactSizeIterator for Values<'_, K, V> {}
impl<K, V> core::iter::FusedIterator for Values<'_, K, V> {}

impl<'a, K, V> IntoIterator for &'a BTreeMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;
    fn into_iter(self) -> Self::IntoIter { self.iter() }
}
impl<'a, K, V> IntoIterator for &'a mut BTreeMap<K, V> {
    type Item = (&'a K, &'a mut V);
    type IntoIter = IterMut<'a, K, V>;
    fn into_iter(self) -> Self::IntoIter { self.iter_mut() }
}
impl<K: Ord, V, const N: usize> From<[(K, V); N]> for BTreeMap<K, V> {
    fn from(entries: [(K, V); N]) -> Self { entries.into_iter().collect() }
}
impl<K: Hash, V: Hash> Hash for BTreeMap<K, V> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Stable Hasher protocol for the collection length prefix.
        state.write_usize(self.len());
        for entry in self { entry.hash(state); }
    }
}
impl<K: Ord, V> core::iter::Extend<(K, V)> for BTreeMap<K, V> {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, entries: I) {
        for (key, value) in entries { self.insert(key, value); }
    }
}
impl<K: Ord, V> core::iter::FromIterator<(K, V)> for BTreeMap<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(entries: I) -> Self {
        let mut map = Self::new();
        map.extend(entries);
        map
    }
}
impl<K: PartialEq, V: PartialEq> PartialEq for BTreeMap<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().zip(other.iter()).all(|((a, x), (b, y))| a == b && x == y)
    }
}
impl<K: Eq, V: Eq> Eq for BTreeMap<K, V> {}
impl<K: core::fmt::Debug, V: core::fmt::Debug> core::fmt::Debug for BTreeMap<K, V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

/// A vacant or occupied slot held under the map's exclusive borrow.
pub enum Entry<'a, K, V> {
    Vacant(VacantEntry<'a, K, V>),
    Occupied(OccupiedEntry<'a, K, V>),
}
pub struct VacantEntry<'a, K, V> { map: &'a mut BTreeMap<K, V>, key: K }
pub struct OccupiedEntry<'a, K, V> { map: &'a mut BTreeMap<K, V>, index: usize, path: Vec<bool> }

impl<K: Ord, V> BTreeMap<K, V> {
    pub fn entry(&mut self, key: K) -> Entry<'_, K, V> {
        let mut index = self.root;
        let mut path = Vec::new();
        while index != NONE {
            let node = &self.nodes[index];
            match key.cmp(&node.entry.as_ref().unwrap().0) {
                Ordering::Equal => return Entry::Occupied(OccupiedEntry { map: self, index, path }),
                Ordering::Less => { path.push(true); index = node.left; },
                Ordering::Greater => { path.push(false); index = node.right; },
            }
        }
        Entry::Vacant(VacantEntry { map: self, key })
    }
}
impl<'a, K: Ord, V> Entry<'a, K, V> {
    pub fn key(&self) -> &K {
        match self { Entry::Vacant(entry) => entry.key(), Entry::Occupied(entry) => entry.key() }
    }
    pub fn or_insert(self, default: V) -> &'a mut V {
        match self { Entry::Vacant(entry) => entry.insert(default), Entry::Occupied(entry) => entry.into_mut() }
    }
    pub fn or_insert_with<F: FnOnce() -> V>(self, default: F) -> &'a mut V {
        match self { Entry::Vacant(entry) => entry.insert(default()), Entry::Occupied(entry) => entry.into_mut() }
    }
    pub fn or_insert_with_key<F: FnOnce(&K) -> V>(self, default: F) -> &'a mut V {
        match self {
            Entry::Vacant(entry) => { let value = default(entry.key()); entry.insert(value) }
            Entry::Occupied(entry) => entry.into_mut(),
        }
    }
    pub fn or_default(self) -> &'a mut V where V: Default { self.or_insert_with(V::default) }
    pub fn and_modify<F: FnOnce(&mut V)>(self, f: F) -> Self {
        match self {
            Entry::Vacant(entry) => Entry::Vacant(entry),
            Entry::Occupied(mut entry) => { f(entry.get_mut()); Entry::Occupied(entry) },
        }
    }
}
impl<'a, K: Ord, V> VacantEntry<'a, K, V> {
    pub fn key(&self) -> &K { &self.key }
    pub fn into_key(self) -> K { self.key }
    pub fn insert(self, value: V) -> &'a mut V {
        let (root, _, index) = self.map.insert_at(self.map.root, self.key, value);
        self.map.root = root;
        &mut self.map.nodes[index].entry.as_mut().unwrap().1
    }
}
impl<'a, K: Ord, V> OccupiedEntry<'a, K, V> {
    pub fn remove(self) -> V { self.remove_entry().1 }
    pub fn remove_entry(self) -> (K, V) {
        let (root, entry) = self.map.remove_path(self.map.root, &self.path);
        self.map.root = root;
        entry.unwrap()
    }
    pub fn key(&self) -> &K { &self.map.nodes[self.index].entry.as_ref().unwrap().0 }
    pub fn get(&self) -> &V { &self.map.nodes[self.index].entry.as_ref().unwrap().1 }
    pub fn get_mut(&mut self) -> &mut V { &mut self.map.nodes[self.index].entry.as_mut().unwrap().1 }
    pub fn into_mut(self) -> &'a mut V { &mut self.map.nodes[self.index].entry.as_mut().unwrap().1 }
    pub fn insert(&mut self, value: V) -> V {
        core::mem::replace(&mut self.map.nodes[self.index].entry.as_mut().unwrap().1, value)
    }
}

impl<K, V> IntoIter<K, V> {
    fn remaining(&self) -> Iter<'_, K, V> {
        Iter { map: &self.map, stack: self.stack.clone(), back_stack: self.back_stack.clone(), remaining: self.map.len }
    }
}
impl<K: core::fmt::Debug, V: core::fmt::Debug> core::fmt::Debug for Iter<'_, K, V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.debug_list().entries(self.clone()).finish() }
}
impl<K: core::fmt::Debug, V: core::fmt::Debug> core::fmt::Debug for IterMut<'_, K, V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.order.as_slice().iter().map(|index| {
            let (key, value) = self.entries[*index].as_ref().unwrap();
            (*key, &**value)
        })).finish()
    }
}
impl<K: core::fmt::Debug, V: core::fmt::Debug> core::fmt::Debug for IntoIter<K, V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.debug_list().entries(self.remaining()).finish() }
}
impl<K: core::fmt::Debug, V> core::fmt::Debug for Keys<'_, K, V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.debug_list().entries(self.clone()).finish() }
}
impl<K, V: core::fmt::Debug> core::fmt::Debug for Values<'_, K, V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.debug_list().entries(self.clone()).finish() }
}
impl<K, V: core::fmt::Debug> core::fmt::Debug for ValuesMut<'_, K, V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.inner.order.as_slice().iter().map(|index| {
            &*self.inner.entries[*index].as_ref().unwrap().1
        })).finish()
    }
}
impl<K, V: core::fmt::Debug> core::fmt::Debug for IntoValues<K, V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.debug_list().entries(self.inner.remaining().map(|(_, value)| value)).finish() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validate(map: &BTreeMap<i32, i32>) {
        fn visit(map: &BTreeMap<i32, i32>, index: usize, lower: Option<i32>, upper: Option<i32>, seen: &mut [bool]) -> usize {
            if index == NONE { return 0; }
            assert!(!seen[index], "tree link repeats slot {index}");
            seen[index] = true;
            let node = &map.nodes[index];
            let key = node.entry.as_ref().unwrap().0;
            assert!(lower.is_none_or(|bound| bound < key));
            assert!(upper.is_none_or(|bound| key < bound));
            let left = visit(map, node.left, lower, Some(key), seen);
            let right = visit(map, node.right, Some(key), upper, seen);
            assert!(left.abs_diff(right) <= 1, "unbalanced key {key}: {left}, {right}");
            assert_eq!(node.height, 1 + left.max(right));
            node.height
        }
        let mut seen = vec![false; map.nodes.len()];
        visit(map, map.root, None, None, &mut seen);
        assert_eq!(seen.iter().filter(|&&live| live).count(), map.len());
        let mut free = map.free;
        while free != NONE {
            assert!(!seen[free], "free link repeats occupied slot {free}");
            seen[free] = true;
            assert!(map.nodes[free].entry.is_none());
            free = map.nodes[free].right;
        }
        assert!(seen.iter().all(|&reachable| reachable));
    }

    #[test]
    fn mixed_removal_entry_and_reuse_preserve_avl_invariants() {
        let mut map = BTreeMap::new();
        let mut reference = std::collections::BTreeMap::new();
        let mut state = 17u32;
        for step in 0..12000 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let key = ((state >> 8) % 257) as i32;
            match state & 3 {
                0 => assert_eq!(map.remove_entry(&key), reference.remove_entry(&key)),
                1 => assert_eq!(map.insert(key, step), reference.insert(key, step)),
                _ => {
                    *map.entry(key).and_modify(|value| *value += 1).or_default() += 3;
                    *reference.entry(key).and_modify(|value| *value += 1).or_default() += 3;
                }
            }
            validate(&map);
            assert_eq!(map.iter().collect::<Vec<_>>(), reference.iter().collect::<Vec<_>>());
        }
        let slots = map.nodes.len();
        for key in 0..257 { assert_eq!(map.remove(&key), reference.remove(&key)); validate(&map); }
        assert!(map.is_empty());
        for key in 0..128 { map.insert(key, key); validate(&map); }
        assert_eq!(map.nodes.len(), slots, "insertion must reuse vacant slots");
    }

    #[test]
    fn comparator_and_entry_callback_panics_leave_map_usable() {
        use std::cell::Cell;
        use std::panic::{AssertUnwindSafe, catch_unwind};
        struct Key<'a> { value: i32, remaining: &'a Cell<usize> }
        impl PartialEq for Key<'_> { fn eq(&self, other: &Self) -> bool { self.value == other.value } }
        impl Eq for Key<'_> {}
        impl PartialOrd for Key<'_> { fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) } }
        impl Ord for Key<'_> {
            fn cmp(&self, other: &Self) -> Ordering {
                let remaining = self.remaining.get();
                assert_ne!(remaining, 0, "comparison panic");
                self.remaining.set(remaining - 1);
                self.value.cmp(&other.value)
            }
        }
        for quota in 0..8 {
            let remaining = Cell::new(usize::MAX);
            let mut map = BTreeMap::new();
            for value in 0..257 { map.insert(Key { value, remaining: &remaining }, value); }
            remaining.set(quota);
            let removed = catch_unwind(AssertUnwindSafe(|| map.remove(&Key { value: 149, remaining: &remaining })));
            remaining.set(usize::MAX);
            assert_eq!(map.len(), if removed.is_err() { 257 } else { 256 });
            let values: Vec<_> = map.iter().map(|(key, &value)| (key.value, value)).collect();
            assert!(values.windows(2).all(|pair| pair[0].0 < pair[1].0));
            for (key, value) in values { assert_eq!(map.get(&Key { value: key, remaining: &remaining }), Some(&value)); }
        }
        let mut map = BTreeMap::new();
        map.insert(1, 2);
        assert!(catch_unwind(AssertUnwindSafe(|| { map.entry(3).or_insert_with(|| panic!("initializer panic")); })).is_err());
        assert_eq!(map.len(), 1);
        assert!(catch_unwind(AssertUnwindSafe(|| { map.entry(1).and_modify(|value| { *value = 9; panic!("callback panic"); }); })).is_err());
        assert_eq!(map.get(&1), Some(&9));
        validate(&map);
    }

    #[test]
    fn removed_key_drop_panics_after_tree_is_consistent() {
        use std::cell::Cell;
        use std::panic::{AssertUnwindSafe, catch_unwind};
        struct Key<'a> { rank: i32, tag: i32, armed: &'a Cell<bool>, drops: &'a Cell<usize> }
        impl Borrow<i32> for Key<'_> { fn borrow(&self) -> &i32 { &self.rank } }
        impl PartialEq for Key<'_> { fn eq(&self, other: &Self) -> bool { self.rank == other.rank } }
        impl Eq for Key<'_> {}
        impl PartialOrd for Key<'_> { fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) } }
        impl Ord for Key<'_> { fn cmp(&self, other: &Self) -> Ordering { self.rank.cmp(&other.rank) } }
        impl Drop for Key<'_> {
            fn drop(&mut self) {
                self.drops.set(self.drops.get() + 1);
                if self.rank == 15 && self.armed.replace(false) { panic!("key destructor panic"); }
            }
        }
        let armed = Cell::new(false);
        let drops = Cell::new(0);
        let mut map = BTreeMap::new();
        for rank in 0..31 { map.insert(Key { rank, tag: 0, armed: &armed, drops: &drops }, rank); }
        let called = Cell::new(false);
        map.entry(Key { rank: 0, tag: 1, armed: &armed, drops: &drops }).or_insert_with(|| { called.set(true); 99 });
        assert!(!called.get());
        assert_eq!(drops.get(), 1);
        assert_eq!(map.first_key_value().unwrap().0.tag, 0);
        armed.set(true);
        assert!(catch_unwind(AssertUnwindSafe(|| map.remove(&15))).is_err());
        assert_eq!(map.len(), 30);
        assert_eq!(drops.get(), 2);
        assert!(!map.contains_key(&15));
        for rank in 0..31 { if rank != 15 { assert_eq!(map.get(&rank), Some(&rank)); } }
        map.insert(Key { rank: 15, tag: 2, armed: &armed, drops: &drops }, 150);
        assert_eq!(map.len(), 31);
        assert_eq!(map.get(&15), Some(&150));
        drop(map);
        assert_eq!(drops.get(), 33);
    }

    #[test]
    fn retain_and_range_preserve_tree_and_unwinding_invariants() {
        use std::panic::{AssertUnwindSafe, catch_unwind};
        use core::ops::Bound::{Included, Excluded};
        let mut map = BTreeMap::new();
        let mut reference = std::collections::BTreeMap::new();
        for i in (0..1023).rev() { map.insert(i, i); reference.insert(i, i); }
        for divisor in [3, 7, 13, 2] {
            map.retain(|key, value| { *value += 3; key % divisor != 0 });
            reference.retain(|key, value| { *value += 3; key % divisor != 0 });
            validate(&map);
            assert_eq!(map.iter().collect::<Vec<_>>(), reference.iter().collect::<Vec<_>>());
        }
        assert!(catch_unwind(AssertUnwindSafe(|| map.retain(|key, value| {
            *value += 1;
            if *key > 400 { panic!("retain callback"); }
            key % 5 != 0
        }))).is_err());
        assert!(catch_unwind(AssertUnwindSafe(|| reference.retain(|key, value| {
            *value += 1;
            if *key > 400 { panic!("retain callback"); }
            key % 5 != 0
        }))).is_err());
        validate(&map);
        assert_eq!(map.iter().collect::<Vec<_>>(), reference.iter().collect::<Vec<_>>());
        for lower in 0..32 {
            for upper in lower..32 {
                for (left, right) in [(Included(lower), Included(upper)), (Included(lower), Excluded(upper)), (Excluded(lower), Included(upper)), (Excluded(lower), Excluded(upper))] {
                    if lower == upper && matches!((left, right), (Excluded(_), Excluded(_))) { continue; }
                    let expected: Vec<_> = reference.range((left, right)).collect();
                    assert_eq!(map.range((left, right)).collect::<Vec<_>>(), expected);
                    let mut iter = map.range((left, right));
                    let mut host = reference.range((left, right));
                    while host.clone().next().is_some() {
                        assert_eq!(iter.next(), host.next());
                        assert_eq!(iter.next_back(), host.next_back());
                    }
                    assert!(iter.next().is_none());
                    assert!(iter.next_back().is_none());
                }
            }
        }
        for bounds in [(Included(9), Included(2)), (Excluded(4), Excluded(4))] {
            assert!(catch_unwind(|| map.range(bounds)).is_err());
            assert!(catch_unwind(|| reference.range(bounds)).is_err());
        }
        map.clear(); reference.clear();
        assert_eq!(map.range((Excluded(4), Excluded(4))).count(), 0);
        validate(&map);
        map.insert(1, 1); validate(&map);
    }

    #[test]
    fn retained_value_drop_panics_after_tree_links_are_consistent() {
        use std::cell::Cell;
        use std::panic::{AssertUnwindSafe, catch_unwind};
        struct Value<'a> { key: i32, armed: &'a Cell<bool>, drops: &'a Cell<usize> }
        impl Drop for Value<'_> {
            fn drop(&mut self) {
                self.drops.set(self.drops.get() + 1);
                if self.key == 5 && self.armed.replace(false) { panic!("value destructor"); }
            }
        }
        let armed = Cell::new(true);
        let drops = Cell::new(0);
        let mut map = BTreeMap::new();
        for key in 0..31 { map.insert(key, Value { key, armed: &armed, drops: &drops }); }
        assert!(catch_unwind(AssertUnwindSafe(|| map.retain(|key, _| key % 2 == 0))).is_err());
        assert_eq!(map.len(), 28);
        assert_eq!(drops.get(), 3);
        assert!(!map.contains_key(&1) && !map.contains_key(&3) && !map.contains_key(&5));
        assert_eq!(map.last_key_value().unwrap().0, &30);
        map.insert(33, Value { key: 33, armed: &armed, drops: &drops });
        drop(map);
        assert_eq!(drops.get(), 32);
    }

    #[test]
    fn occupied_removal_does_not_compare_after_lookup() {
        use std::cell::Cell;
        struct Key<'a> { value: i32, allowed: &'a Cell<bool> }
        impl PartialEq for Key<'_> { fn eq(&self, other: &Self) -> bool { self.value == other.value } }
        impl Eq for Key<'_> {}
        impl PartialOrd for Key<'_> { fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) } }
        impl Ord for Key<'_> {
            fn cmp(&self, other: &Self) -> Ordering {
                assert!(self.allowed.get(), "occupied removal called Ord");
                self.value.cmp(&other.value)
            }
        }
        let allowed = Cell::new(true);
        let mut map = BTreeMap::new();
        for value in 0..127 { map.insert(Key { value, allowed: &allowed }, value); }
        for value in [63, 31, 95, 0, 126] {
            let entry = match map.entry(Key { value, allowed: &allowed }) {
                Entry::Occupied(entry) => entry,
                Entry::Vacant(_) => panic!("missing key"),
            };
            allowed.set(false);
            let removed = entry.remove_entry();
            allowed.set(true);
            assert_eq!((removed.0.value, removed.1), (value, value));
            assert!(!map.contains_key(&Key { value, allowed: &allowed }));
            assert!(map.iter().map(|(key, _)| key.value).collect::<Vec<_>>().windows(2).all(|w| w[0] < w[1]));
        }
    }

    #[test]
    fn append_comparison_and_replaced_value_panics_preserve_ownership() {
        use std::cell::Cell;
        use std::panic::{AssertUnwindSafe, catch_unwind};
        struct Key<'a> { value: i32, remaining: &'a Cell<usize> }
        impl PartialEq for Key<'_> { fn eq(&self, other: &Self) -> bool { self.value == other.value } }
        impl Eq for Key<'_> {}
        impl PartialOrd for Key<'_> { fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) } }
        impl Ord for Key<'_> {
            fn cmp(&self, other: &Self) -> Ordering {
                let remaining = self.remaining.get();
                assert!(remaining != 0, "comparison once");
                self.remaining.set(remaining - 1);
                self.value.cmp(&other.value)
            }
        }
        struct Value<'a> { value: i32, bomb: &'a Cell<bool>, drops: &'a Cell<usize> }
        impl Drop for Value<'_> {
            fn drop(&mut self) {
                self.drops.set(self.drops.get() + 1);
                if self.value == 5 && self.bomb.replace(false) { panic!("drop once"); }
            }
        }
        for quota in 0..9 {
            let remaining = Cell::new(usize::MAX);
            let bomb = Cell::new(false);
            let drops = Cell::new(0);
            let mut left = BTreeMap::new();
            let mut right = BTreeMap::new();
            for value in 0..15 { left.insert(Key { value, remaining: &remaining }, Value { value, bomb: &bomb, drops: &drops }); }
            for value in 7..22 { right.insert(Key { value, remaining: &remaining }, Value { value: value + 100, bomb: &bomb, drops: &drops }); }
            remaining.set(quota);
            let _ = catch_unwind(AssertUnwindSafe(|| left.append(&mut right)));
            remaining.set(usize::MAX);
            assert!(right.is_empty());
            let keys: Vec<_> = left.keys().map(|key| key.value).collect();
            assert!(keys.windows(2).all(|w| w[0] < w[1]));
            for key in keys { assert!(left.contains_key(&Key { value: key, remaining: &remaining })); }
            drop(left); drop(right);
            assert_eq!(drops.get(), 30);
        }
        let remaining = Cell::new(usize::MAX);
        let bomb = Cell::new(false);
        let drops = Cell::new(0);
        let mut left = BTreeMap::new();
        let mut right = BTreeMap::new();
        left.insert(Key { value: 5, remaining: &remaining }, Value { value: 5, bomb: &bomb, drops: &drops });
        for value in 4..7 { right.insert(Key { value, remaining: &remaining }, Value { value: value + 100, bomb: &bomb, drops: &drops }); }
        bomb.set(true);
        assert!(catch_unwind(AssertUnwindSafe(|| left.append(&mut right))).is_err());
        assert!(right.is_empty());
        assert_eq!(left.get(&Key { value: 5, remaining: &remaining }).unwrap().value, 105);
        drop(left); drop(right); assert_eq!(drops.get(), 4);
    }

    #[test]
    fn mutable_iterator_debug_panic_does_not_consume_entries() {
        use std::cell::Cell;
        use std::panic::{AssertUnwindSafe, catch_unwind};
        struct Value<'a>(&'a Cell<bool>, i32);
        impl core::fmt::Debug for Value<'_> {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                assert!(!self.0.replace(false), "format once");
                self.1.fmt(f)
            }
        }
        let bomb = Cell::new(false);
        let mut map = BTreeMap::new();
        for key in 0..7 { map.insert(key, Value(&bomb, key)); }
        let mut iter = map.iter_mut();
        iter.next().unwrap().1.1 += 10;
        iter.next_back().unwrap().1.1 += 20;
        bomb.set(true);
        assert!(catch_unwind(AssertUnwindSafe(|| format!("{iter:?}"))).is_err());
        assert_eq!(iter.len(), 5);
        let references: Vec<_> = iter.collect();
        for (key, value) in references { value.1 += *key; }
        assert_eq!(map.values().map(|v| v.1).collect::<Vec<_>>(), [10, 2, 4, 6, 8, 10, 26]);
    }

}

/// A borrowed, double-ended view over an ordered key interval.
pub struct Range<'a, K, V> {
    map: &'a BTreeMap<K, V>,
    front: Vec<usize>,
    back: Vec<usize>,
    finished: bool,
}
impl<K, V> Clone for Range<'_, K, V> {
    fn clone(&self) -> Self {
        Self { map: self.map, front: self.front.clone(), back: self.back.clone(), finished: self.finished }
    }
}
impl<'a, K, V> Iterator for Range<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        if self.finished { return None; }
        let index = self.front.pop()?;
        if self.back.last() == Some(&index) { self.finished = true; }
        let mut at = self.map.nodes[index].right;
        while at != NONE { self.front.push(at); at = self.map.nodes[at].left; }
        self.map.nodes[index].entry.as_ref().map(|(key, value)| (key, value))
    }
}
impl<'a, K, V> core::iter::DoubleEndedIterator for Range<'a, K, V> {
    fn next_back(&mut self) -> Option<(&'a K, &'a V)> {
        if self.finished { return None; }
        let index = self.back.pop()?;
        if self.front.last() == Some(&index) { self.finished = true; }
        let mut at = self.map.nodes[index].left;
        while at != NONE { self.back.push(at); at = self.map.nodes[at].right; }
        self.map.nodes[index].entry.as_ref().map(|(key, value)| (key, value))
    }
}
impl<K, V> core::iter::FusedIterator for Range<'_, K, V> {}
