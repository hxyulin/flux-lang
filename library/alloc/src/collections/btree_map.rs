//! An ordered map with logarithmic lookup and amortized logarithmic insertion.
//!
//! The public subset follows `BTreeMap`, but storage is an AVL tree in a flat
//! vector, not the standard library's private B-tree layout. Links are indices;
//! rotations never move keys or values between slots. Iterators use a stack
//! bounded by tree height. Removed slots are reused through an index free list.
//! Range and allocator APIs remain outside this subset.
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

    pub fn iter(&self) -> Iter<'_, K, V> {
        let mut iter = Iter {
            map: self,
            stack: Vec::new(),
            remaining: self.len,
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
    remaining: usize,
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
        self.remaining -= 1;
        self.descend(node.right);
        let entry = node.entry.as_ref().unwrap();
        Some((&entry.0, &entry.1))
    }
    fn size_hint(&self) -> (usize, Option<usize>) { (self.remaining, Some(self.remaining)) }
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

impl<K, V> BTreeMap<K, V> {
    /// Mutable values in key order. Each node index is visited once; taking
    /// its reference from the slot prevents either iterator end yielding it twice.
    pub fn values_mut(&mut self) -> ValuesMut<'_, K, V> {
        let mut order = Vec::new();
        let mut stack = Vec::new();
        let mut at = self.root;
        while at != NONE || !stack.is_empty() {
            while at != NONE {
                stack.push(at);
                at = self.nodes[at].left;
            }
            at = stack.pop().unwrap();
            order.push(at);
            at = self.nodes[at].right;
        }
        let values = self.nodes.iter_mut().map(|node| node.entry.as_mut().map(|(_, value)| value)).collect();
        ValuesMut { order: order.into_iter(), values, keys: core::marker::PhantomData }
    }
}

pub struct ValuesMut<'a, K, V> {
    order: crate::vec::IntoIter<usize>,
    values: Vec<Option<&'a mut V>>,
    keys: core::marker::PhantomData<&'a mut K>,
}
impl<'a, K, V> core::iter::Iterator for ValuesMut<'a, K, V> {
    type Item = &'a mut V;
    fn next(&mut self) -> Option<&'a mut V> {
        self.order.next().and_then(|index| self.values[index].take())
    }
    fn size_hint(&self) -> (usize, Option<usize>) { self.order.size_hint() }
}
impl<'a, K, V> core::iter::DoubleEndedIterator for ValuesMut<'a, K, V> {
    fn next_back(&mut self) -> Option<&'a mut V> {
        core::iter::DoubleEndedIterator::next_back(&mut self.order).and_then(|index| self.values[index].take())
    }
}
impl<'a, K, V> core::iter::ExactSizeIterator for ValuesMut<'a, K, V> {}
impl<'a, K, V> core::iter::FusedIterator for ValuesMut<'a, K, V> {}

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
        Self { map: self.map, stack: self.stack.clone(), remaining: self.remaining }
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
pub struct OccupiedEntry<'a, K, V> { map: &'a mut BTreeMap<K, V>, index: usize }

impl<K: Ord, V> BTreeMap<K, V> {
    pub fn entry(&mut self, key: K) -> Entry<'_, K, V> {
        let index = self.find(&key);
        if index == NONE { Entry::Vacant(VacantEntry { map: self, key }) }
        else { Entry::Occupied(OccupiedEntry { map: self, index }) }
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
    pub fn key(&self) -> &K { &self.map.nodes[self.index].entry.as_ref().unwrap().0 }
    pub fn get(&self) -> &V { &self.map.nodes[self.index].entry.as_ref().unwrap().1 }
    pub fn get_mut(&mut self) -> &mut V { &mut self.map.nodes[self.index].entry.as_mut().unwrap().1 }
    pub fn into_mut(self) -> &'a mut V { &mut self.map.nodes[self.index].entry.as_mut().unwrap().1 }
    pub fn insert(&mut self, value: V) -> V {
        core::mem::replace(&mut self.map.nodes[self.index].entry.as_mut().unwrap().1, value)
    }
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

}
