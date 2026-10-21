use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::{Index, IndexMut};
use std::sync::Arc;

/// The index of a `T` in an [`Arena<T>`]. Four bytes, `Copy`, and typed,
/// so an index into one arena cannot be used with an arena of another
/// type.
pub struct Idx<T> {
    raw: u32,
    // `fn() -> T` keeps `Idx<T>` `Send`, `Sync` and covariant whatever `T`
    // is: an index does not own a `T`.
    _ty: PhantomData<fn() -> T>,
}

impl<T> Idx<T> {
    pub const fn from_raw(raw: u32) -> Idx<T> {
        Idx {
            raw,
            _ty: PhantomData,
        }
    }

    pub const fn into_raw(self) -> u32 {
        self.raw
    }

    pub const fn index(self) -> usize {
        self.raw as usize
    }
}

// Manual impls: derives would require `T: Clone` and so on.
impl<T> Clone for Idx<T> {
    fn clone(&self) -> Idx<T> {
        *self
    }
}

impl<T> Copy for Idx<T> {}

impl<T> PartialEq for Idx<T> {
    fn eq(&self, other: &Idx<T>) -> bool {
        self.raw == other.raw
    }
}

impl<T> Eq for Idx<T> {}

impl<T> PartialOrd for Idx<T> {
    fn partial_cmp(&self, other: &Idx<T>) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Idx<T> {
    fn cmp(&self, other: &Idx<T>) -> std::cmp::Ordering {
        self.raw.cmp(&other.raw)
    }
}

impl<T> Hash for Idx<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}

impl<T> fmt::Debug for Idx<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = std::any::type_name::<T>();
        let name = name.rsplit("::").next().unwrap_or(name);
        write!(f, "Idx::<{name}>({})", self.raw)
    }
}

/// Values of one type, named by [`Idx`]. Allocation is a vector push;
/// values are never removed individually.
///
/// ```
/// use flux_arena::Arena;
/// let mut exprs = Arena::new();
/// let one = exprs.alloc("1");
/// let two = exprs.alloc("2");
/// assert_eq!((exprs[one], exprs[two]), ("1", "2"));
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct Arena<T> {
    data: Vec<T>,
}

impl<T> Default for Arena<T> {
    fn default() -> Arena<T> {
        Arena { data: Vec::new() }
    }
}

impl<T> Arena<T> {
    pub const fn new() -> Arena<T> {
        Arena { data: Vec::new() }
    }

    pub fn with_capacity(capacity: usize) -> Arena<T> {
        Arena {
            data: Vec::with_capacity(capacity),
        }
    }

    /// Adds `value` and returns its index.
    ///
    /// # Panics
    ///
    /// If the arena already holds `u32::MAX` values.
    #[inline]
    pub fn alloc(&mut self, value: T) -> Idx<T> {
        let idx = self.next_idx();
        self.data.push(value);
        idx
    }

    /// The index the next allocation will get.
    #[inline]
    pub fn next_idx(&self) -> Idx<T> {
        let raw = u32::try_from(self.data.len()).expect("arena holds more than u32::MAX values");
        Idx::from_raw(raw)
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn get(&self, idx: Idx<T>) -> Option<&T> {
        self.data.get(idx.index())
    }

    pub fn get_mut(&mut self, idx: Idx<T>) -> Option<&mut T> {
        self.data.get_mut(idx.index())
    }

    /// Every value with its index, in allocation order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (Idx<T>, &T)> + DoubleEndedIterator {
        self.data
            .iter()
            .enumerate()
            .map(|(i, value)| (Idx::from_raw(i as u32), value))
    }

    pub fn values(&self) -> std::slice::Iter<'_, T> {
        self.data.iter()
    }

    pub fn values_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.data.iter_mut()
    }

    /// The values, in allocation order.
    pub fn into_values(self) -> std::vec::IntoIter<T> {
        self.data.into_iter()
    }

    pub fn shrink_to_fit(&mut self) {
        self.data.shrink_to_fit();
    }
}

impl<T> Index<Idx<T>> for Arena<T> {
    type Output = T;

    #[inline]
    fn index(&self, idx: Idx<T>) -> &T {
        &self.data[idx.index()]
    }
}

impl<T> IndexMut<Idx<T>> for Arena<T> {
    #[inline]
    fn index_mut(&mut self, idx: Idx<T>) -> &mut T {
        &mut self.data[idx.index()]
    }
}

impl<T: fmt::Debug> fmt::Debug for Arena<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entries(self.iter().map(|(i, v)| (i.raw, v)))
            .finish()
    }
}

/// An [`Arena<T>`] whose first values are *frozen*: shared, through an
/// `Arc`, with every copy of the arena, and never changed. Values
/// allocated after freezing are the arena's own and get the next
/// indices, so copying an arena that continues a large frozen one costs
/// only its own values.
///
/// ```
/// use flux_arena::LayeredArena;
/// let mut base = LayeredArena::new();
/// let one = base.alloc("1");
/// base.freeze();
/// let mut copy = base.clone();
/// let two = copy.alloc("2");
/// assert_eq!((copy[one], copy[two]), ("1", "2"));
/// assert_eq!((base.len(), copy.len()), (1, 2));
/// ```
#[derive(Clone)]
pub struct LayeredArena<T> {
    frozen: Arc<[T]>,
    data: Vec<T>,
}

impl<T> Default for LayeredArena<T> {
    fn default() -> LayeredArena<T> {
        LayeredArena {
            frozen: Arc::new([]),
            data: Vec::new(),
        }
    }
}

impl<T> LayeredArena<T> {
    pub fn new() -> LayeredArena<T> {
        LayeredArena::default()
    }

    /// Adds `value` and returns its index.
    ///
    /// # Panics
    ///
    /// If the arena already holds `u32::MAX` values.
    #[inline]
    pub fn alloc(&mut self, value: T) -> Idx<T> {
        let raw = u32::try_from(self.len()).expect("arena holds more than u32::MAX values");
        self.data.push(value);
        Idx::from_raw(raw)
    }

    /// Freezes every value: from now on they are shared by copies.
    pub fn freeze(&mut self)
    where
        T: Clone,
    {
        if self.data.is_empty() {
            return;
        }
        self.frozen = if self.frozen.is_empty() {
            std::mem::take(&mut self.data).into()
        } else {
            self.frozen
                .iter()
                .cloned()
                .chain(self.data.drain(..))
                .collect()
        };
    }

    pub fn len(&self) -> usize {
        self.frozen.len() + self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn get(&self, idx: Idx<T>) -> Option<&T> {
        let i = idx.index();
        match i.checked_sub(self.frozen.len()) {
            None => Some(&self.frozen[i]),
            Some(own) => self.data.get(own),
        }
    }

    /// Every value with its index, in allocation order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (Idx<T>, &T)> + DoubleEndedIterator {
        self.values()
            .enumerate()
            .map(|(i, value)| (Idx::from_raw(i as u32), value))
    }

    pub fn values(&self) -> impl ExactSizeIterator<Item = &T> + DoubleEndedIterator {
        Values {
            frozen: self.frozen.iter(),
            data: self.data.iter(),
        }
    }
}

/// The values of a [`LayeredArena`]: `Chain`, but with an exact size.
struct Values<'a, T> {
    frozen: std::slice::Iter<'a, T>,
    data: std::slice::Iter<'a, T>,
}

impl<'a, T> Iterator for Values<'a, T> {
    type Item = &'a T;

    #[inline]
    fn next(&mut self) -> Option<&'a T> {
        self.frozen.next().or_else(|| self.data.next())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.frozen.len() + self.data.len();
        (n, Some(n))
    }

    fn nth(&mut self, n: usize) -> Option<&'a T> {
        let skip = n.min(self.frozen.len());
        self.frozen.nth(skip).or_else(|| self.data.nth(n - skip))
    }
}

impl<T> DoubleEndedIterator for Values<'_, T> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.data.next_back().or_else(|| self.frozen.next_back())
    }
}

impl<T> ExactSizeIterator for Values<'_, T> {}

impl<T> Index<Idx<T>> for LayeredArena<T> {
    type Output = T;

    #[inline]
    fn index(&self, idx: Idx<T>) -> &T {
        let i = idx.index();
        match i.checked_sub(self.frozen.len()) {
            None => &self.frozen[i],
            Some(own) => &self.data[own],
        }
    }
}

impl<T> IndexMut<Idx<T>> for LayeredArena<T> {
    /// # Panics
    ///
    /// If the value is frozen.
    #[inline]
    fn index_mut(&mut self, idx: Idx<T>) -> &mut T {
        let own = idx
            .index()
            .checked_sub(self.frozen.len())
            .expect("frozen values cannot be changed");
        &mut self.data[own]
    }
}

impl<T: fmt::Debug> fmt::Debug for LayeredArena<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entries(self.iter().map(|(i, v)| (i.raw, v)))
            .finish()
    }
}

/// Extra data for some values of an [`Arena<T>`], keyed by their indices.
/// Dense: memory is proportional to the largest index stored.
#[derive(Clone, PartialEq, Eq)]
pub struct ArenaMap<T, V> {
    data: Vec<Option<V>>,
    _ty: PhantomData<fn() -> T>,
}

impl<T, V> Default for ArenaMap<T, V> {
    fn default() -> ArenaMap<T, V> {
        ArenaMap {
            data: Vec::new(),
            _ty: PhantomData,
        }
    }
}

impl<T, V> ArenaMap<T, V> {
    pub fn new() -> ArenaMap<T, V> {
        ArenaMap::default()
    }

    /// Sets the value for `idx`, returning the previous one.
    pub fn insert(&mut self, idx: Idx<T>, value: V) -> Option<V> {
        let i = idx.index();
        if i >= self.data.len() {
            self.data.resize_with(i + 1, || None);
        }
        self.data[i].replace(value)
    }

    pub fn get(&self, idx: Idx<T>) -> Option<&V> {
        self.data.get(idx.index())?.as_ref()
    }

    pub fn get_mut(&mut self, idx: Idx<T>) -> Option<&mut V> {
        self.data.get_mut(idx.index())?.as_mut()
    }

    pub fn remove(&mut self, idx: Idx<T>) -> Option<V> {
        self.data.get_mut(idx.index())?.take()
    }

    pub fn contains(&self, idx: Idx<T>) -> bool {
        self.get(idx).is_some()
    }

    /// The stored entries, in index order.
    pub fn iter(&self) -> impl Iterator<Item = (Idx<T>, &V)> {
        self.data
            .iter()
            .enumerate()
            .filter_map(|(i, v)| Some((Idx::from_raw(i as u32), v.as_ref()?)))
    }
}

impl<T, V> Index<Idx<T>> for ArenaMap<T, V> {
    type Output = V;

    fn index(&self, idx: Idx<T>) -> &V {
        self.get(idx)
            .expect("no value for this index in the ArenaMap")
    }
}

impl<T, V: fmt::Debug> fmt::Debug for ArenaMap<T, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entries(self.iter().map(|(i, v)| (i.raw, v)))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indices_are_dense_and_stable() {
        let mut arena = Arena::new();
        let a = arena.alloc(String::from("a"));
        let b = arena.alloc(String::from("b"));
        arena[a].push('!');
        assert_eq!((a.into_raw(), b.into_raw()), (0, 1));
        assert_eq!(
            arena.iter().map(|(_, s)| s.as_str()).collect::<Vec<_>>(),
            ["a!", "b"]
        );
        assert_eq!(format!("{a:?}"), "Idx::<String>(0)");
    }

    #[test]
    fn map_attaches_data_sparsely() {
        let mut arena = Arena::new();
        let ids: Vec<_> = (0..5).map(|i| arena.alloc(i)).collect();
        let mut types: ArenaMap<i32, &str> = ArenaMap::new();
        assert_eq!(types.insert(ids[3], "u8"), None);
        assert_eq!(types.insert(ids[3], "u16"), Some("u8"));
        types.insert(ids[1], "bool");
        assert_eq!(types[ids[3]], "u16");
        assert!(!types.contains(ids[2]));
        assert_eq!(
            types
                .iter()
                .map(|(i, v)| (i.index(), *v))
                .collect::<Vec<_>>(),
            [(1, "bool"), (3, "u16")]
        );
        assert_eq!(types.remove(ids[1]), Some("bool"));
    }

    #[test]
    fn layered_arena_shares_frozen_values() {
        let mut base = LayeredArena::new();
        let a = base.alloc(String::from("a"));
        base.freeze();
        let mut copy = base.clone();
        let b = copy.alloc(String::from("b"));
        copy[b].push('!');
        copy.freeze();
        let c = copy.alloc(String::from("c"));
        let all: Vec<_> = copy.iter().map(|(i, s)| (i.index(), s.as_str())).collect();
        assert_eq!(all, [(0, "a"), (1, "b!"), (2, "c")]);
        assert_eq!(copy.values().len(), 3);
        assert_eq!(copy.values().nth(2).map(String::as_str), Some("c"));
        assert_eq!(copy.values().rev().nth(2).map(String::as_str), Some("a"));
        assert_eq!(
            (copy.get(a), copy.get(Idx::from_raw(3))),
            (Some(&base[a]), None)
        );
        assert_eq!((base.len(), copy[c].as_str()), (1, "c"));
        let frozen = std::panic::catch_unwind(move || copy[a].push('?'));
        assert!(frozen.is_err());
    }

    #[test]
    fn idx_is_small_and_thread_safe() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Idx<std::rc::Rc<u8>>>();
        assert_eq!(std::mem::size_of::<Idx<String>>(), 4);
        assert_eq!(std::mem::size_of::<Option<Idx<String>>>(), 8);
    }
}
