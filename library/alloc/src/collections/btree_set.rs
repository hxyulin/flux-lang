//! Ordered set storage shares the map's balanced tree and key ownership.
use super::btree_map;
use super::BTreeMap;
use core::borrow::Borrow;

pub struct BTreeSet<T> {
    map: BTreeMap<T, ()>,
}

impl<T> BTreeSet<T> {
    pub const fn new() -> Self { Self { map: BTreeMap::new() } }
    pub fn len(&self) -> usize { self.map.len() }
    pub fn is_empty(&self) -> bool { self.map.is_empty() }
    pub fn clear(&mut self) { self.map.clear(); }
    pub fn iter(&self) -> Iter<'_, T> { Iter { inner: self.map.iter() } }
    pub fn contains<Q: ?Sized>(&self, value: &Q) -> bool
    where T: Borrow<Q> + Ord, Q: Ord {
        self.map.contains_key(value)
    }
}
impl<T: Ord> BTreeSet<T> {
    pub fn remove<Q: ?Sized>(&mut self, value: &Q) -> bool
    where T: Borrow<Q>, Q: Ord { self.map.remove(value).is_some() }
    pub fn pop_first(&mut self) -> Option<T> { self.map.pop_first().map(|(key, _)| key) }
    pub fn retain<F: FnMut(&T) -> bool>(&mut self, mut f: F) { self.map.retain(|key, _| f(key)); }
    pub fn insert(&mut self, value: T) -> bool { self.map.insert(value, ()).is_none() }
}
impl<T> Default for BTreeSet<T> {
    fn default() -> Self { Self::new() }
}
impl<T: Clone> Clone for BTreeSet<T> {
    fn clone(&self) -> Self { Self { map: self.map.clone() } }
}
impl<T: PartialEq> PartialEq for BTreeSet<T> {
    fn eq(&self, other: &Self) -> bool { self.map == other.map }
}
impl<T: Eq> Eq for BTreeSet<T> {}
impl<T: core::fmt::Debug> core::fmt::Debug for BTreeSet<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}
impl<T: Ord> core::iter::Extend<T> for BTreeSet<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, values: I) {
        for value in values { self.insert(value); }
    }
}
impl<T: Ord> core::iter::FromIterator<T> for BTreeSet<T> {
    fn from_iter<I: IntoIterator<Item = T>>(values: I) -> Self {
        let mut set = Self::new();
        set.extend(values);
        set
    }
}
impl<T: Ord, const N: usize> From<[T; N]> for BTreeSet<T> {
    fn from(values: [T; N]) -> Self { values.into_iter().collect() }
}
pub struct Iter<'a, T> { inner: btree_map::Iter<'a, T, ()> }
impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<&'a T> { self.inner.next().map(|(key, _)| key) }
    fn size_hint(&self) -> (usize, Option<usize>) { self.inner.size_hint() }
}
impl<'a, T> core::iter::DoubleEndedIterator for Iter<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> { self.inner.next_back().map(|(key, _)| key) }
}
impl<T> Clone for Iter<'_, T> { fn clone(&self) -> Self { Self { inner: self.inner.clone() } } }
impl<T> core::iter::ExactSizeIterator for Iter<'_, T> {}
impl<T> core::iter::FusedIterator for Iter<'_, T> {}
impl<'a, T> IntoIterator for &'a BTreeSet<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter { self.iter() }
}
pub struct IntoIter<T> { inner: btree_map::IntoIter<T, ()> }
impl<T> Iterator for IntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> { self.inner.next().map(|(key, _)| key) }
}
impl<T> core::iter::FusedIterator for IntoIter<T> {}
impl<T> IntoIterator for BTreeSet<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;
    fn into_iter(self) -> IntoIter<T> { IntoIter { inner: self.map.into_iter() } }
}
