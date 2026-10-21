use std::cell::{Cell, RefCell};
use std::fmt;
use std::mem::{self, MaybeUninit};
use std::ptr::{self, NonNull};
use std::slice;

/// The unit chunks are made of. Its alignment is the largest the arena
/// supports, so every chunk starts suitably aligned for any value.
#[repr(C, align(16))]
struct Block([u8; 16]);

const MAX_ALIGN: usize = mem::align_of::<Block>();
const BLOCK: usize = mem::size_of::<Block>();
const FIRST_CHUNK: usize = 4 * 1024;
const MAX_CHUNK: usize = 2 * 1024 * 1024;
/// Allocations at least this large get a chunk of their own, so they do
/// not waste the rest of the current chunk.
const DEDICATED: usize = MAX_CHUNK / 4;

/// A bump allocator for values that need no destructor.
///
/// Allocation takes `&self` and returns a reference tied to the arena, so
/// many allocations can be alive at once. Memory is requested in chunks
/// that double in size up to 2 MiB; an allocation is a bounds check and a
/// pointer increment. Everything is freed when the arena is dropped.
///
/// Only `Copy` types are accepted, which guarantees that nothing needs to
/// be dropped. Alignments up to 16 bytes are supported.
///
/// ```
/// let arena = flux_arena::DroplessArena::default();
/// let name: &str = arena.alloc_str("main");
/// let numbers: &mut [u32] = arena.alloc_slice(&[1, 2, 3]);
/// numbers[0] = 7;
/// assert_eq!((name, &*numbers), ("main", &[7, 2, 3][..]));
/// ```
pub struct DroplessArena {
    /// The next free byte in the current chunk.
    ptr: Cell<*mut u8>,
    /// The end of the current chunk.
    end: Cell<*mut u8>,
    /// Every chunk, as returned by `Box::into_raw`. Freed on drop.
    chunks: RefCell<Vec<NonNull<[MaybeUninit<Block>]>>>,
}

// SAFETY: The arena exclusively owns its chunks, and references it hands
// out borrow the arena, so none can outlive a move to another thread. It is
// deliberately not `Sync`: allocation mutates `ptr` through a `Cell`.
unsafe impl Send for DroplessArena {}

impl Default for DroplessArena {
    fn default() -> DroplessArena {
        DroplessArena {
            ptr: Cell::new(ptr::null_mut()),
            end: Cell::new(ptr::null_mut()),
            chunks: RefCell::new(Vec::new()),
        }
    }
}

impl DroplessArena {
    pub fn new() -> DroplessArena {
        DroplessArena::default()
    }

    /// Moves `value` into the arena.
    #[inline]
    #[allow(clippy::mut_from_ref)]
    pub fn alloc<T: Copy>(&self, value: T) -> &mut T {
        const { assert!(mem::align_of::<T>() <= MAX_ALIGN) };
        if mem::size_of::<T>() == 0 {
            // SAFETY: A dangling, aligned pointer is a valid reference to a
            // zero-sized value, and zero-sized values cannot overlap.
            return unsafe { &mut *NonNull::<T>::dangling().as_ptr() };
        }
        let p = self
            .alloc_raw(mem::size_of::<T>(), mem::align_of::<T>())
            .cast::<T>();
        // SAFETY: `alloc_raw` returned memory valid for writes of
        // `size_of::<T>()` bytes, aligned for `T`, that no other allocation
        // overlaps and that lives until the arena is dropped.
        unsafe {
            p.as_ptr().write(value);
            &mut *p.as_ptr()
        }
    }

    /// Copies `src` into the arena.
    #[inline]
    #[allow(clippy::mut_from_ref)]
    pub fn alloc_slice<T: Copy>(&self, src: &[T]) -> &mut [T] {
        const { assert!(mem::align_of::<T>() <= MAX_ALIGN) };
        if src.is_empty() || mem::size_of::<T>() == 0 {
            // SAFETY: A dangling, aligned pointer is valid for a slice of
            // any length when the slice occupies no memory.
            return unsafe {
                slice::from_raw_parts_mut(NonNull::<T>::dangling().as_ptr(), src.len())
            };
        }
        let p = self
            .alloc_raw(mem::size_of_val(src), mem::align_of::<T>())
            .cast::<T>();
        // SAFETY: `alloc_raw` returned memory valid for writes of the whole
        // slice, aligned for `T` and overlapping no other allocation (so not
        // `src` either). `T: Copy`, so a bitwise copy is a valid copy.
        unsafe {
            ptr::copy_nonoverlapping(src.as_ptr(), p.as_ptr(), src.len());
            slice::from_raw_parts_mut(p.as_ptr(), src.len())
        }
    }

    /// Copies `s` into the arena.
    #[inline]
    pub fn alloc_str(&self, s: &str) -> &str {
        let bytes = self.alloc_slice(s.as_bytes());
        // SAFETY: The bytes were copied from a `str`, so they are UTF-8.
        unsafe { std::str::from_utf8_unchecked(bytes) }
    }

    /// Bytes reserved from the system allocator, including unused space at
    /// the end of chunks.
    pub fn capacity_bytes(&self) -> usize {
        self.chunks.borrow().iter().map(|c| c.len() * BLOCK).sum()
    }

    /// Returns `size > 0` bytes aligned to `align`, a power of two up to
    /// `MAX_ALIGN`.
    #[inline]
    fn alloc_raw(&self, size: usize, align: usize) -> NonNull<u8> {
        debug_assert!(size > 0 && align.is_power_of_two() && align <= MAX_ALIGN);
        let ptr = self.ptr.get();
        let available = self.end.get().addr() - ptr.addr();
        // `align_offset` may report `usize::MAX` when it cannot compute an
        // offset; that falls through to a new chunk, which is aligned.
        let offset = ptr.align_offset(align);
        if offset <= available && size <= available - offset {
            // SAFETY: `offset + size <= available`, so both the start and the
            // new bump pointer stay inside the current chunk or one past its
            // end. A chunk exists, because `available > 0`.
            unsafe {
                let start = ptr.add(offset);
                self.ptr.set(start.add(size));
                return NonNull::new_unchecked(start);
            }
        }
        self.alloc_in_new_chunk(size)
    }

    #[cold]
    #[inline(never)]
    fn alloc_in_new_chunk(&self, size: usize) -> NonNull<u8> {
        let mut chunks = self.chunks.borrow_mut();
        let previous = chunks.last().map_or(0, |c| c.len() * BLOCK);
        let chunk_size = if size >= DEDICATED {
            size
        } else {
            (previous * 2).clamp(FIRST_CHUNK, MAX_CHUNK)
        };
        let blocks = chunk_size.div_ceil(BLOCK);
        let chunk = NonNull::from(Box::leak(Box::<[Block]>::new_uninit_slice(blocks)));
        chunks.push(chunk);
        let start = chunk.cast::<u8>().as_ptr();
        if size < DEDICATED {
            // SAFETY: The chunk is `blocks * BLOCK >= size` bytes long, so
            // both pointers are inside it or one past its end.
            unsafe {
                self.ptr.set(start.add(size));
                self.end.set(start.add(blocks * BLOCK));
            }
        }
        // SAFETY: `start` comes from a live allocation, so it is not null.
        unsafe { NonNull::new_unchecked(start) }
    }
}

impl Drop for DroplessArena {
    fn drop(&mut self) {
        for chunk in self.chunks.get_mut().drain(..) {
            // SAFETY: Each chunk came from `Box::leak` and is freed exactly
            // once. Its contents are `Copy` and need no drop. References into
            // it borrowed the arena, which is being dropped, so none remain.
            drop(unsafe { Box::from_raw(chunk.as_ptr()) });
        }
    }
}

impl fmt::Debug for DroplessArena {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DroplessArena")
            .field("chunks", &self.chunks.borrow().len())
            .field("capacity_bytes", &self.capacity_bytes())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_slices_and_strings_survive_further_allocation() {
        let arena = DroplessArena::new();
        let a = arena.alloc(1u8);
        let b = arena.alloc(2u64);
        let c = arena.alloc_slice(&[3u16, 4, 5]);
        let s = arena.alloc_str("héllo");
        // Force several new chunks while the references above are alive.
        let many: Vec<&mut u32> = (0..10_000).map(|i| arena.alloc(i)).collect();
        *a += 10;
        *b += 10;
        c[0] += 10;
        assert_eq!((*a, *b, &*c, s), (11, 12, &[13, 4, 5][..], "héllo"));
        assert!(many.iter().enumerate().all(|(i, v)| **v == i as u32));
    }

    #[test]
    fn respects_alignment() {
        #[derive(Clone, Copy)]
        #[repr(align(16))]
        struct Aligned(#[allow(dead_code)] u8);
        let arena = DroplessArena::new();
        for i in 0..100u8 {
            arena.alloc(i);
            let p = arena.alloc(Aligned(i)) as *mut Aligned;
            assert_eq!(p.addr() % 16, 0);
            let q = arena.alloc(i as u64) as *mut u64;
            assert_eq!(q.addr() % 8, 0);
        }
    }

    #[test]
    fn large_allocations_get_their_own_chunk() {
        let arena = DroplessArena::new();
        let small = arena.alloc(1u8);
        let big = vec![7u8; DEDICATED + 1];
        let copy = arena.alloc_slice(&big);
        // The current chunk is still used after the dedicated one.
        let after = arena.alloc(2u8) as *mut u8;
        assert_eq!(after.addr(), (small as *mut u8).addr() + 1);
        assert_eq!(copy, &big[..]);
    }

    #[test]
    fn zero_sized_and_empty() {
        let arena = DroplessArena::new();
        arena.alloc(());
        assert!(arena.alloc_slice::<u32>(&[]).is_empty());
        assert_eq!(arena.alloc_slice(&[(), ()]).len(), 2);
        assert_eq!(arena.alloc_str(""), "");
        assert_eq!(arena.capacity_bytes(), 0);
    }

    #[test]
    fn can_move_between_threads() {
        let arena = DroplessArena::new();
        arena.alloc_str("before");
        let arena = std::thread::spawn(move || {
            arena.alloc_str("after");
            arena
        })
        .join()
        .unwrap();
        assert_eq!(arena.alloc_str("back"), "back");
    }
}
