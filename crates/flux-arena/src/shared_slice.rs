//! Owned immutable slices sharing reference-counted allocation chunks.

use std::cell::UnsafeCell;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::mem::MaybeUninit;
use std::ops::{Deref, Range};
use std::sync::{Arc, Weak};

const CHUNK_ELEMENTS: usize = 256;

struct Chunk<T> {
    data: Box<[UnsafeCell<MaybeUninit<T>>]>,
}

// SAFETY: Only the pool writes to the chunk, through its exclusive borrow,
// and only to slots never previously exposed. Published slices are immutable
// and keep their chunk alive. Moving a chunk requires its elements to be Send.
unsafe impl<T: Send> Send for Chunk<T> {}
// SAFETY: Readers access only initialized, immutable slots. The pool's
// exclusive cursor prevents writes from overlapping those slots. Shared
// access to the published elements requires T: Sync.
unsafe impl<T: Send + Sync> Sync for Chunk<T> {}

/// Batches immutable `Copy` slices into chunks. Unlike a reference arena,
/// each returned slice owns its storage and can outlive the pool.
///
/// The pool retains only a weak reference to its current chunk: dropping
/// all slices releases the storage immediately, even if the pool is cached
/// on a thread. Large slices get dedicated chunks.
pub struct SharedSlicePool<T: Copy> {
    current: Weak<Chunk<T>>,
    used: usize,
}

impl<T: Copy> Default for SharedSlicePool<T> {
    fn default() -> Self {
        Self {
            current: Weak::new(),
            used: 0,
        }
    }
}

impl<T: Copy> SharedSlicePool<T> {
    /// Copies values directly into a chunk, without an intermediate vector.
    pub fn alloc<I>(&mut self, values: I) -> SharedSlice<T>
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: ExactSizeIterator,
    {
        let values = values.into_iter();
        let len = values.len();
        let len32 = u32::try_from(len).expect("slice has more than u32::MAX elements");
        if len == 0 {
            assert_eq!(values.count(), 0, "incorrect iterator length");
            return SharedSlice::default();
        }
        let (chunk, start) = if len > CHUNK_ELEMENTS {
            (Self::chunk(len), 0)
        } else {
            let chunk = self
                .current
                .upgrade()
                .filter(|c| len <= c.data.len() - self.used);
            let chunk = chunk.unwrap_or_else(|| {
                let chunk = Self::chunk(CHUNK_ELEMENTS);
                self.current = Arc::downgrade(&chunk);
                self.used = 0;
                chunk
            });
            let start = self.used;
            // Reserve before iterating, so unwinding cannot reuse slots.
            self.used += len;
            (chunk, start)
        };
        let mut written = 0;
        for value in values {
            // ExactSizeIterator is safe to implement, so a wrong length
            // must panic instead of causing an out-of-bounds write.
            assert!(written < len, "incorrect iterator length");
            // SAFETY: The slot is in the reserved range, is not exposed by
            // any slice, and this pool exclusively owns the write cursor.
            unsafe {
                (*chunk.data[start + written].get()).write(value);
            }
            written += 1;
        }
        assert_eq!(written, len, "incorrect iterator length");
        SharedSlice {
            chunk: Some(chunk),
            start: start as u32,
            len: len32,
        }
    }

    fn chunk(len: usize) -> Arc<Chunk<T>> {
        Arc::new(Chunk {
            data: std::iter::repeat_with(|| UnsafeCell::new(MaybeUninit::uninit()))
                .take(len)
                .collect(),
        })
    }
}

/// An immutable slice whose clones and subslices share one allocation.
/// Empty slices allocate nothing. Dropping the last slice of a chunk frees
/// it; `Copy` elements require no individual destructors.
pub struct SharedSlice<T: Copy> {
    chunk: Option<Arc<Chunk<T>>>,
    start: u32,
    len: u32,
}

impl<T: Copy> SharedSlice<T> {
    pub fn slice(&self, range: Range<usize>) -> Self {
        assert!(range.start <= range.end && range.end <= self.len as usize);
        if range.is_empty() {
            return Self::default();
        }
        Self {
            chunk: self.chunk.clone(),
            start: self.start + range.start as u32,
            len: (range.end - range.start) as u32,
        }
    }
}

impl<T: Copy> Default for SharedSlice<T> {
    fn default() -> Self {
        Self {
            chunk: None,
            start: 0,
            len: 0,
        }
    }
}

impl<T: Copy> Clone for SharedSlice<T> {
    fn clone(&self) -> Self {
        Self {
            chunk: self.chunk.clone(),
            start: self.start,
            len: self.len,
        }
    }
}

impl<T: Copy> Deref for SharedSlice<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        let Some(chunk) = &self.chunk else { return &[] };
        // SAFETY: UnsafeCell and MaybeUninit have T's layout. Construction
        // initializes every slot of this range before exposing the slice,
        // and no published slot is ever written again. The Arc keeps the
        // allocation alive for the lifetime of this borrow.
        unsafe {
            std::slice::from_raw_parts(
                chunk.data.as_ptr().cast::<T>().add(self.start as usize),
                self.len as usize,
            )
        }
    }
}

impl<T: Copy> AsRef<[T]> for SharedSlice<T> {
    fn as_ref(&self) -> &[T] {
        self
    }
}

impl<T: Copy + fmt::Debug> fmt::Debug for SharedSlice<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(f)
    }
}

impl<T: Copy + PartialEq> PartialEq for SharedSlice<T> {
    fn eq(&self, other: &Self) -> bool {
        self.deref() == other.deref()
    }
}

impl<T: Copy + Eq> Eq for SharedSlice<T> {}

impl<T: Copy + Hash> Hash for SharedSlice<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.deref().hash(state);
    }
}

impl<'a, T: Copy> IntoIterator for &'a SharedSlice<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batches_slices_and_releases_the_last_chunk() {
        let mut pool = SharedSlicePool::default();
        let a = pool.alloc([1u32, 2]);
        let b = pool.alloc([3u32, 4]);
        assert!(Arc::ptr_eq(
            a.chunk.as_ref().unwrap(),
            b.chunk.as_ref().unwrap()
        ));
        let weak = pool.current.clone();
        let copy = a.clone();
        drop((a, b));
        assert_eq!(&*copy, &[1, 2]);
        drop(copy);
        assert!(weak.upgrade().is_none());
        assert_eq!(&*pool.alloc([5]), &[5]);
    }

    #[test]
    fn survives_pool_drop_and_chunk_rollover() {
        let mut pool = SharedSlicePool::default();
        let first = pool.alloc([1usize, 2, 3]);
        for i in 0..CHUNK_ELEMENTS {
            assert_eq!(pool.alloc([i])[0], i);
        }
        let large = pool.alloc(0..CHUNK_ELEMENTS + 1);
        let part = first.slice(1..3);
        drop((pool, first));
        assert_eq!(&*part, &[2, 3]);
        assert_eq!(large.len(), CHUNK_ELEMENTS + 1);
    }

    #[test]
    fn readers_can_run_while_the_pool_appends() {
        let mut pool = SharedSlicePool::default();
        let a = pool.alloc([7u32; 4]);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                for _ in 0..100 {
                    assert_eq!(&*a, &[7; 4]);
                }
            });
            for i in 0..100 {
                assert_eq!(pool.alloc([i])[0], i);
            }
        });
    }

    #[test]
    fn empty_and_zero_sized_slices() {
        let mut pool = SharedSlicePool::<()>::default();
        assert!(pool.alloc([]).is_empty());
        assert!(pool.current.upgrade().is_none());
        assert_eq!(pool.alloc([(); 300]).len(), 300);
    }

    #[test]
    fn incorrect_iterator_lengths_cannot_expose_uninitialized_slots() {
        struct Wrong(std::ops::Range<u32>, usize);
        impl Iterator for Wrong {
            type Item = u32;
            fn next(&mut self) -> Option<u32> {
                self.0.next()
            }
            fn size_hint(&self) -> (usize, Option<usize>) {
                (self.1, Some(self.1))
            }
        }
        impl ExactSizeIterator for Wrong {}
        let mut pool = SharedSlicePool::default();
        for iter in [Wrong(0..2, 1), Wrong(0..1, 2)] {
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| pool.alloc(iter)))
                    .is_err()
            );
        }
        assert_eq!(&*pool.alloc([9]), &[9]);
    }
}
