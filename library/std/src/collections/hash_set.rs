//! Sets sharing the map's hashing, collision and ownership rules.
use super::hash_map::{self, HashMap};
use core::borrow::Borrow;
use core::hash::{BuildHasher, Hash};
use core::iter::FusedIterator;

#[derive(Clone)]
pub struct HashSet<T, S = crate::hash::RandomState> { map: HashMap<T, (), S> }
impl<T> HashSet<T, crate::hash::RandomState> {
    pub fn new() -> Self { Self::with_hasher(crate::hash::RandomState::new()) }
    pub fn with_capacity(capacity: usize) -> Self {
        let mut set = Self::new(); set.reserve(capacity); set
    }
}
impl<T, S> HashSet<T, S> {
    pub const fn with_hasher(hasher: S) -> Self { Self { map: HashMap::with_hasher(hasher) } }
    pub fn len(&self) -> usize { self.map.len() }
    pub fn is_empty(&self) -> bool { self.map.is_empty() }
    pub fn capacity(&self) -> usize { self.map.capacity() }
    pub fn clear(&mut self) { self.map.clear(); }
    pub fn reserve(&mut self, additional: usize) { self.map.reserve(additional); }
    pub fn iter(&self) -> Iter<'_, T> { Iter { inner: self.map.keys() } }
    pub fn retain<F: FnMut(&T) -> bool>(&mut self, mut f: F) { self.map.retain(|key, _| f(key)); }
}
impl<T, S: Default> Default for HashSet<T, S> {
    fn default() -> Self { Self::with_hasher(S::default()) }
}
impl<T: Eq + Hash, S: BuildHasher> HashSet<T, S> {
    pub fn with_capacity_and_hasher(capacity: usize, hasher: S) -> Self {
        Self { map: HashMap::with_capacity_and_hasher(capacity, hasher) }
    }
    pub fn insert(&mut self, value: T) -> bool { self.map.insert(value, ()).is_none() }
    pub fn contains<Q: ?Sized + Eq + Hash>(&self, value: &Q) -> bool where T: Borrow<Q> {
        self.map.contains_key(value)
    }
    pub fn get<Q: ?Sized + Eq + Hash>(&self, value: &Q) -> Option<&T> where T: Borrow<Q> {
        self.map.get_key_value(value).map(|(key, _)| key)
    }
    pub fn remove<Q: ?Sized + Eq + Hash>(&mut self, value: &Q) -> bool where T: Borrow<Q> {
        self.map.remove(value).is_some()
    }
    pub fn take<Q: ?Sized + Eq + Hash>(&mut self, value: &Q) -> Option<T> where T: Borrow<Q> {
        self.map.remove_entry(value).map(|(key, _)| key)
    }
    pub fn is_subset(&self, other: &Self) -> bool { self.iter().all(|value| other.contains(value)) }
    pub fn is_disjoint(&self, other: &Self) -> bool { self.iter().all(|value| !other.contains(value)) }
}
impl<T: Eq + Hash, S: BuildHasher> PartialEq for HashSet<T, S> {
    fn eq(&self, other: &Self) -> bool { self.len() == other.len() && self.is_subset(other) }
}
impl<T: Eq + Hash, S: BuildHasher> Eq for HashSet<T, S> {}
impl<T: core::fmt::Debug, S> core::fmt::Debug for HashSet<T, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.debug_set().entries(self.iter()).finish() }
}
impl<T: Eq + Hash, S: BuildHasher + Default> FromIterator<T> for HashSet<T, S> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self { let mut set = Self::default(); set.extend(iter); set }
}
impl<T: Eq + Hash, S: BuildHasher> Extend<T> for HashSet<T, S> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) { for value in iter { self.insert(value); } }
}
pub struct Iter<'a, T> { inner: hash_map::Keys<'a, T, ()> }
impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> { self.inner.next() }
    fn size_hint(&self) -> (usize, Option<usize>) { self.inner.size_hint() }
}
impl<T> ExactSizeIterator for Iter<'_, T> {}
impl<T> FusedIterator for Iter<'_, T> {}
pub struct IntoIter<T> { inner: hash_map::IntoIter<T, ()> }
impl<T> Iterator for IntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> { self.inner.next().map(|(key, _)| key) }
    fn size_hint(&self) -> (usize, Option<usize>) { self.inner.size_hint() }
}
impl<T> ExactSizeIterator for IntoIter<T> {}
impl<T> FusedIterator for IntoIter<T> {}
impl<T, S> IntoIterator for HashSet<T, S> {
    type Item = T; type IntoIter = IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter { IntoIter { inner: self.map.into_iter() } }
}
impl<'a, T, S> IntoIterator for &'a HashSet<T, S> {
    type Item = &'a T; type IntoIter = Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter { self.iter() }
}

impl<'a,T: 'a+Copy+Eq+Hash,S:BuildHasher> Extend<&'a T> for HashSet<T,S> {
    fn extend<I:IntoIterator<Item=&'a T>>(&mut self,iter:I) {for &value in iter {self.insert(value);}}
}

impl<T:Eq+Hash,const N:usize> From<[T;N]> for HashSet<T> {
    fn from(values:[T;N])->Self {values.into_iter().collect()}
}
