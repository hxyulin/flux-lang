use std::mem::{self, MaybeUninit};

/// Chunk storage for immutable process-lifetime slices of `Copy` values.
/// Chunks deliberately remain allocated when the allocator is dropped.
/// This suits global interners: small slices share one allocation.
pub struct StaticSliceArena<T: Copy + 'static> {
    remaining: &'static mut [MaybeUninit<T>],
}

impl<T: Copy + 'static> Default for StaticSliceArena<T> {
    fn default() -> Self {
        Self { remaining: &mut [] }
    }
}

impl<T: Copy + 'static> StaticSliceArena<T> {
    pub const fn new() -> Self {
        Self { remaining: &mut [] }
    }

    pub fn alloc(&mut self, values: &[T]) -> &'static [T] {
        if values.is_empty() {
            return &[];
        }
        if self.remaining.len() < values.len() {
            let capacity = (4096 / mem::size_of::<T>().max(1)).max(values.len());
            self.remaining = Box::leak(Box::<[T]>::new_uninit_slice(capacity));
        }
        let (allocated, remaining) = mem::take(&mut self.remaining).split_at_mut(values.len());
        self.remaining = remaining;
        for (slot, value) in allocated.iter_mut().zip(values) {
            slot.write(*value);
        }
        // SAFETY: Every element was initialized above. The leaked chunk lives
        // forever, and splitting removes these slots from all future writes.
        unsafe { std::slice::from_raw_parts(allocated.as_ptr().cast::<T>(), allocated.len()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_survive_chunks_and_allocator() {
        // Keep process-lifetime allocations reachable, as an interner does.
        static ROOTS: std::sync::Mutex<Vec<&'static [u64]>> = std::sync::Mutex::new(Vec::new());
        let first;
        {
            let mut arena = StaticSliceArena::default();
            first = arena.alloc(&[1u64, 2]);
            let next = arena.alloc(&[3u64]);
            assert_eq!(next.as_ptr(), first.as_ptr().wrapping_add(2));
            let large = arena.alloc(&vec![4; 2048]);
            assert_eq!(large, &[4; 2048]);
            ROOTS.lock().unwrap().extend([first, next, large]);
            assert_eq!(first, &[1, 2]);
        }
        assert_eq!(first, &[1, 2]);
    }

    #[test]
    fn empty_zero_sized_and_aligned() {
        assert!(StaticSliceArena::<u8>::default().alloc(&[]).is_empty());
        assert_eq!(StaticSliceArena::default().alloc(&[(); 3]), &[(); 3]);
        #[derive(Clone, Copy)]
        #[repr(align(64))]
        struct Aligned(u8);
        static ROOT: std::sync::Mutex<Option<&'static [Aligned]>> = std::sync::Mutex::new(None);
        let values = StaticSliceArena::default().alloc(&[Aligned(7)]);
        *ROOT.lock().unwrap() = Some(values);
        assert_eq!(values.as_ptr().addr() % 64, 0);
        assert_eq!(values[0].0, 7);
    }
}
