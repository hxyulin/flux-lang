//! Flat storage for typed, contiguous child lists.
use std::{fmt, marker::PhantomData, ops::Index};

/// A typed range into a [`SlicePool`], independent of its allocation address.
pub struct SliceId<T> {
    start: u32,
    end: u32,
    marker: PhantomData<fn() -> T>,
}
impl<T> Copy for SliceId<T> {}
impl<T> Clone for SliceId<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> PartialEq for SliceId<T> {
    fn eq(&self, other: &Self) -> bool {
        self.start == other.start && self.end == other.end
    }
}
impl<T> Eq for SliceId<T> {}
impl<T> Default for SliceId<T> {
    fn default() -> Self {
        Self {
            start: 0,
            end: 0,
            marker: PhantomData,
        }
    }
}
impl<T> fmt::Debug for SliceId<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}
impl<T> SliceId<T> {
    pub fn len(self) -> usize {
        (self.end - self.start) as usize
    }
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}
/// All lists of one element type, freed together. No references escape mutation.
#[derive(Debug)]
pub struct SlicePool<T> {
    values: Vec<T>,
}
impl<T> Default for SlicePool<T> {
    fn default() -> Self {
        Self { values: Vec::new() }
    }
}
impl<T> SlicePool<T> {
    pub fn alloc(&mut self, values: impl IntoIterator<Item = T>) -> SliceId<T> {
        let start = u32::try_from(self.values.len()).expect("slice pool exceeds u32::MAX elements");
        self.values.extend(values);
        let end = u32::try_from(self.values.len()).expect("slice pool exceeds u32::MAX elements");
        SliceId {
            start,
            end,
            marker: PhantomData,
        }
    }
}
impl<T> Index<SliceId<T>> for SlicePool<T> {
    type Output = [T];
    fn index(&self, id: SliceId<T>) -> &[T] {
        &self.values[id.start as usize..id.end as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranges_survive_growth_and_preserve_empty_and_owned_lists() {
        let mut pool = SlicePool::default();
        let first = pool.alloc([String::from("first"), String::from("second")]);
        let empty = pool.alloc([]);
        let later = pool.alloc((0..1000).map(|i| i.to_string()));
        assert_eq!(&pool[first], &["first", "second"]);
        assert!(pool[empty].is_empty());
        assert_eq!(pool[later].last().unwrap(), "999");
        assert_eq!(first.len(), 2);
        assert_eq!(std::mem::size_of_val(&first), 8);
    }
}

#[cfg(test)]
mod scratch_tests {
    use super::*;

    #[test]
    fn draining_nested_lists_keeps_parent_entries_and_reuses_scratch() {
        let mut pool = SlicePool::default();
        let mut scratch = Vec::with_capacity(8);
        scratch.push(10);
        let nested_start = scratch.len();
        scratch.extend([20, 30]);
        let nested = pool.alloc(scratch.drain(nested_start..));
        assert_eq!(&scratch, &[10]);
        scratch.push(40);
        let parent = pool.alloc(scratch.drain(..));
        assert_eq!(&pool[nested], &[20, 30]);
        assert_eq!(&pool[parent], &[10, 40]);
        assert!(scratch.is_empty());
        assert_eq!(scratch.capacity(), 8);
    }
}
