//! Linear probing with cached hashes, tombstones and keyed default hashing.
use crate::alloc_crate::vec::{IntoIter as VecIntoIter, Vec};
pub use crate::hash::DefaultHasher;
use core::borrow::Borrow;
use core::hash::{BuildHasher, Hash};
use core::iter::FusedIterator;
use core::ops::Index;
use core::slice;

#[derive(Clone)]
enum Bucket<K, V> {
    Empty,
    Deleted,
    Full(u64, K, V),
}

#[derive(Clone)]
pub struct HashMap<K, V, S = crate::hash::RandomState> {
    buckets: Vec<Bucket<K, V>>,
    len: usize,
    deleted: usize,
    hash_builder: S,
}

impl<K, V> HashMap<K, V, crate::hash::RandomState> {
    pub fn new() -> Self {
        Self::with_hasher(crate::hash::RandomState::new())
    }
    pub fn with_capacity(capacity: usize) -> Self {
        let mut map = Self::new();
        map.reserve(capacity);
        map
    }
}

impl<K, V, S> HashMap<K, V, S> {
    pub const fn with_hasher(hash_builder: S) -> Self {
        Self {
            buckets: Vec::new(),
            len: 0,
            deleted: 0,
            hash_builder,
        }
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn capacity(&self) -> usize {
        self.buckets.len() / 4 * 3
    }
    pub fn hasher(&self) -> &S {
        &self.hash_builder
    }
    pub fn clear(&mut self) {
        for bucket in &mut self.buckets {
            let old = core::mem::replace(bucket, Bucket::Empty);
            match &old {
                Bucket::Full(..) => self.len -= 1,
                Bucket::Deleted => self.deleted -= 1,
                Bucket::Empty => {}
            }
            drop(old);
        }
    }
    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter {
            inner: self.buckets.iter(),
            remaining: self.len,
        }
    }
    pub fn iter_mut(&mut self) -> IterMut<'_, K, V> {
        IterMut {
            inner: self.buckets.iter_mut(),
            remaining: self.len,
        }
    }
    pub fn keys(&self) -> Keys<'_, K, V> {
        Keys { inner: self.iter() }
    }
    pub fn values(&self) -> Values<'_, K, V> {
        Values { inner: self.iter() }
    }
    pub fn values_mut(&mut self) -> ValuesMut<'_, K, V> {
        ValuesMut {
            inner: self.iter_mut(),
        }
    }
    pub fn into_keys(self) -> IntoKeys<K, V> {
        IntoKeys {
            inner: self.into_iter(),
        }
    }
    pub fn into_values(self) -> IntoValues<K, V> {
        IntoValues {
            inner: self.into_iter(),
        }
    }
    pub fn retain<F: FnMut(&K, &mut V) -> bool>(&mut self, mut keep: F) {
        for bucket in &mut self.buckets {
            let remove = match bucket {
                Bucket::Full(_, key, value) => !keep(key, value),
                _ => false,
            };
            if remove {
                let old = core::mem::replace(bucket, Bucket::Deleted);
                self.len -= 1;
                self.deleted += 1;
                drop(old);
            }
        }
    }
    fn vacant(&self, hash: u64) -> usize {
        let mut index = hash as usize & (self.buckets.len() - 1);
        loop {
            if !matches!(self.buckets[index], Bucket::Full(..)) {
                return index;
            }
            index = (index + 1) & (self.buckets.len() - 1);
        }
    }
    fn rehash(&mut self, size: usize) {
        let mut next = Vec::with_capacity(size);
        next.resize_with(size, || Bucket::Empty);
        let old = core::mem::replace(&mut self.buckets, next);
        self.deleted = 0;
        for bucket in old {
            if let Bucket::Full(hash, key, value) = bucket {
                let index = self.vacant(hash);
                self.buckets[index] = Bucket::Full(hash, key, value);
            }
        }
    }
    pub fn reserve(&mut self, additional: usize) {
        let required = self.len.checked_add(additional).expect("capacity overflow");
        let mut size = self.buckets.len().max(8);
        while size / 4 * 3 < required {
            size = size.checked_mul(2).expect("capacity overflow");
        }
        if size != self.buckets.len() || self.len + self.deleted + additional > size / 4 * 3 {
            self.rehash(size);
        }
    }
    fn insert_vacant(&mut self, index: usize, hash: u64, key: K, value: V) -> &mut V {
        if matches!(self.buckets[index], Bucket::Deleted) {
            self.deleted -= 1;
        }
        self.buckets[index] = Bucket::Full(hash, key, value);
        self.len += 1;
        match &mut self.buckets[index] {
            Bucket::Full(_, _, value) => value,
            _ => unreachable!(),
        }
    }
}

impl<K, V, S: Default> Default for HashMap<K, V, S> {
    fn default() -> Self {
        Self::with_hasher(S::default())
    }
}

impl<K: Eq + Hash, V, S: BuildHasher> HashMap<K, V, S> {
    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: S) -> Self {
        let mut map = Self::with_hasher(hash_builder);
        map.reserve(capacity);
        map
    }
    fn find<Q: ?Sized + Eq>(&self, hash: u64, key: &Q) -> Option<usize>
    where
        K: Borrow<Q>,
    {
        if self.buckets.is_empty() {
            return None;
        }
        let mut index = hash as usize & (self.buckets.len() - 1);
        loop {
            match &self.buckets[index] {
                Bucket::Empty => return None,
                Bucket::Full(stored, k, _) if *stored == hash && k.borrow() == key => {
                    return Some(index);
                }
                _ => {}
            }
            index = (index + 1) & (self.buckets.len() - 1);
        }
    }
    pub fn get<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        self.get_key_value(key).map(|(_, value)| value)
    }
    pub fn get_key_value<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> Option<(&K, &V)>
    where
        K: Borrow<Q>,
    {
        let index = self.find(self.hash_builder.hash_one(key), key)?;
        match &self.buckets[index] {
            Bucket::Full(_, key, value) => Some((key, value)),
            _ => unreachable!(),
        }
    }
    pub fn get_mut<Q: ?Sized + Hash + Eq>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
    {
        let index = self.find(self.hash_builder.hash_one(key), key)?;
        match &mut self.buckets[index] {
            Bucket::Full(_, _, value) => Some(value),
            _ => unreachable!(),
        }
    }
    pub fn contains_key<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.get(key).is_some()
    }
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let hash = self.hash_builder.hash_one(&key);
        if let Some(index) = self.find(hash, &key)
            && let Bucket::Full(_, _, old) = &mut self.buckets[index]
        {
            return Some(core::mem::replace(old, value));
        }
        self.reserve(1);
        let index = self.vacant(hash);
        self.insert_vacant(index, hash, key, value);
        None
    }
    pub fn remove<Q: ?Sized + Hash + Eq>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        self.remove_entry(key).map(|(_, value)| value)
    }
    pub fn remove_entry<Q: ?Sized + Hash + Eq>(&mut self, key: &Q) -> Option<(K, V)>
    where
        K: Borrow<Q>,
    {
        let index = self.find(self.hash_builder.hash_one(key), key)?;
        self.len -= 1;
        self.deleted += 1;
        match core::mem::replace(&mut self.buckets[index], Bucket::Deleted) {
            Bucket::Full(_, key, value) => Some((key, value)),
            _ => unreachable!(),
        }
    }
    pub fn entry(&mut self, key: K) -> Entry<'_, K, V, S> {
        let hash = self.hash_builder.hash_one(&key);
        if let Some(index) = self.find(hash, &key) {
            Entry::Occupied(OccupiedEntry { map: self, index })
        } else {
            self.reserve(1);
            let index = self.vacant(hash);
            Entry::Vacant(VacantEntry {
                map: self,
                index,
                hash,
                key,
            })
        }
    }
}

impl<K: Eq + Hash, V, S: BuildHasher + Default> FromIterator<(K, V)> for HashMap<K, V, S> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut map = Self::default();
        map.extend(iter);
        map
    }
}
impl<K: Eq + Hash, V, S: BuildHasher> Extend<(K, V)> for HashMap<K, V, S> {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        for (key, value) in iter {
            self.insert(key, value);
        }
    }
}
impl<K: Eq + Hash, V: PartialEq, S: BuildHasher> PartialEq for HashMap<K, V, S> {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len
            && self
                .iter()
                .all(|(key, value)| other.get(key) == Some(value))
    }
}
impl<K: Eq + Hash, V: Eq, S: BuildHasher> Eq for HashMap<K, V, S> {}
impl<K: core::fmt::Debug, V: core::fmt::Debug, S> core::fmt::Debug for HashMap<K, V, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}
impl<K: Eq + Hash + Borrow<Q>, V, S: BuildHasher, Q: ?Sized + Eq + Hash> Index<&Q>
    for HashMap<K, V, S>
{
    type Output = V;
    fn index(&self, key: &Q) -> &V {
        self.get(key).expect("no entry found for key")
    }
}

pub enum Entry<'a, K, V, S> {
    Occupied(OccupiedEntry<'a, K, V, S>),
    Vacant(VacantEntry<'a, K, V, S>),
}
pub struct OccupiedEntry<'a, K, V, S> {
    map: &'a mut HashMap<K, V, S>,
    index: usize,
}
pub struct VacantEntry<'a, K, V, S> {
    map: &'a mut HashMap<K, V, S>,
    index: usize,
    hash: u64,
    key: K,
}
impl<'a, K, V, S> Entry<'a, K, V, S> {
    pub fn or_insert(self, default: V) -> &'a mut V {
        match self {
            Self::Occupied(entry) => entry.into_mut(),
            Self::Vacant(entry) => entry.insert(default),
        }
    }
    pub fn or_insert_with<F: FnOnce() -> V>(self, default: F) -> &'a mut V {
        match self {
            Self::Occupied(entry) => entry.into_mut(),
            Self::Vacant(entry) => entry.insert(default()),
        }
    }
    pub fn or_default(self) -> &'a mut V
    where
        V: Default,
    {
        self.or_insert_with(V::default)
    }
    pub fn and_modify<F: FnOnce(&mut V)>(mut self, f: F) -> Self {
        if let Self::Occupied(entry) = &mut self {
            f(entry.get_mut());
        }
        self
    }
    pub fn key(&self) -> &K {
        match self {
            Self::Occupied(entry) => entry.key(),
            Self::Vacant(entry) => &entry.key,
        }
    }
}
impl<'a, K, V, S> OccupiedEntry<'a, K, V, S> {
    pub fn key(&self) -> &K {
        match &self.map.buckets[self.index] {
            Bucket::Full(_, key, _) => key,
            _ => unreachable!(),
        }
    }
    pub fn get(&self) -> &V {
        match &self.map.buckets[self.index] {
            Bucket::Full(_, _, value) => value,
            _ => unreachable!(),
        }
    }
    pub fn get_mut(&mut self) -> &mut V {
        match &mut self.map.buckets[self.index] {
            Bucket::Full(_, _, value) => value,
            _ => unreachable!(),
        }
    }
    pub fn into_mut(self) -> &'a mut V {
        match &mut self.map.buckets[self.index] {
            Bucket::Full(_, _, value) => value,
            _ => unreachable!(),
        }
    }
    pub fn insert(&mut self, value: V) -> V {
        core::mem::replace(self.get_mut(), value)
    }
    pub fn remove_entry(self) -> (K, V) {
        self.map.len -= 1;
        self.map.deleted += 1;
        match core::mem::replace(&mut self.map.buckets[self.index], Bucket::Deleted) {
            Bucket::Full(_, key, value) => (key, value),
            _ => unreachable!(),
        }
    }
    pub fn remove(self) -> V {
        self.remove_entry().1
    }
}
impl<'a, K, V, S> VacantEntry<'a, K, V, S> {
    pub fn key(&self) -> &K {
        &self.key
    }
    pub fn into_key(self) -> K {
        self.key
    }
    pub fn insert(self, value: V) -> &'a mut V {
        self.map
            .insert_vacant(self.index, self.hash, self.key, value)
    }
}

pub struct Iter<'a, K, V> {
    inner: slice::Iter<'a, Bucket<K, V>>,
    remaining: usize,
}
impl<'a, K, V> Clone for Iter<'a, K, V> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            remaining: self.remaining,
        }
    }
}
impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        for bucket in self.inner.by_ref() {
            if let Bucket::Full(_, key, value) = bucket {
                self.remaining -= 1;
                return Some((key, value));
            }
        }
        None
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}
impl<K, V> ExactSizeIterator for Iter<'_, K, V> {}
impl<K, V> FusedIterator for Iter<'_, K, V> {}
pub struct IterMut<'a, K, V> {
    inner: slice::IterMut<'a, Bucket<K, V>>,
    remaining: usize,
}
impl<'a, K, V> Iterator for IterMut<'a, K, V> {
    type Item = (&'a K, &'a mut V);
    fn next(&mut self) -> Option<Self::Item> {
        for bucket in self.inner.by_ref() {
            if let Bucket::Full(_, key, value) = bucket {
                self.remaining -= 1;
                return Some((key, value));
            }
        }
        None
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}
impl<K, V> ExactSizeIterator for IterMut<'_, K, V> {}
impl<K, V> FusedIterator for IterMut<'_, K, V> {}
pub struct IntoIter<K, V> {
    inner: VecIntoIter<Bucket<K, V>>,
    remaining: usize,
}
impl<K, V> Iterator for IntoIter<K, V> {
    type Item = (K, V);
    fn next(&mut self) -> Option<Self::Item> {
        for bucket in self.inner.by_ref() {
            if let Bucket::Full(_, key, value) = bucket {
                self.remaining -= 1;
                return Some((key, value));
            }
        }
        None
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}
impl<K, V> ExactSizeIterator for IntoIter<K, V> {}
impl<K, V> FusedIterator for IntoIter<K, V> {}
impl<K, V, S> IntoIterator for HashMap<K, V, S> {
    type Item = (K, V);
    type IntoIter = IntoIter<K, V>;
    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            inner: self.buckets.into_iter(),
            remaining: self.len,
        }
    }
}
impl<'a, K, V, S> IntoIterator for &'a HashMap<K, V, S> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'a, K, V, S> IntoIterator for &'a mut HashMap<K, V, S> {
    type Item = (&'a K, &'a mut V);
    type IntoIter = IterMut<'a, K, V>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

pub struct Keys<'a, K, V> {
    inner: Iter<'a, K, V>,
}
impl<'a, K, V> Iterator for Keys<'a, K, V> {
    type Item = &'a K;
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(key, _)| key)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}
impl<K, V> ExactSizeIterator for Keys<'_, K, V> {}
impl<K, V> FusedIterator for Keys<'_, K, V> {}
pub struct Values<'a, K, V> {
    inner: Iter<'a, K, V>,
}
impl<'a, K, V> Iterator for Values<'a, K, V> {
    type Item = &'a V;
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(_, value)| value)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}
impl<K, V> ExactSizeIterator for Values<'_, K, V> {}
impl<K, V> FusedIterator for Values<'_, K, V> {}
pub struct ValuesMut<'a, K, V> {
    inner: IterMut<'a, K, V>,
}
impl<'a, K, V> Iterator for ValuesMut<'a, K, V> {
    type Item = &'a mut V;
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(_, value)| value)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}
impl<K, V> ExactSizeIterator for ValuesMut<'_, K, V> {}
impl<K, V> FusedIterator for ValuesMut<'_, K, V> {}
pub struct IntoKeys<K, V> {
    inner: IntoIter<K, V>,
}
impl<K, V> Iterator for IntoKeys<K, V> {
    type Item = K;
    fn next(&mut self) -> Option<K> {
        self.inner.next().map(|(key, _)| key)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}
impl<K, V> ExactSizeIterator for IntoKeys<K, V> {}
impl<K, V> FusedIterator for IntoKeys<K, V> {}
pub struct IntoValues<K, V> {
    inner: IntoIter<K, V>,
}
impl<K, V> Iterator for IntoValues<K, V> {
    type Item = V;
    fn next(&mut self) -> Option<V> {
        self.inner.next().map(|(_, value)| value)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}
impl<K, V> ExactSizeIterator for IntoValues<K, V> {}
impl<K, V> FusedIterator for IntoValues<K, V> {}
