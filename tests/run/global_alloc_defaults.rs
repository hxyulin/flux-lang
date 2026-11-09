use std::alloc::{GlobalAlloc, Layout};
use std::cell::Cell;

struct Allocator { frees: Cell<u32> }

// SAFETY: successful allocations and releases delegate to the same system
// allocator with unchanged layouts; large requests fail without touching storage.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() > 128 { return std::ptr::null_mut(); }
        // SAFETY: GlobalAlloc's caller supplies a valid nonzero layout.
        unsafe { std::alloc::alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        self.frees.set(self.frees.get() + 1);
        // SAFETY: the pointer came from this allocator with this layout.
        unsafe { std::alloc::dealloc(pointer, layout); }
    }
}

fn main() {
    let allocator = Allocator { frees: Cell::new(0) };
    let initial = Layout::from_size_align(32, 64).unwrap();
    // SAFETY: each operation uses its live allocation's current layout. Successful
    // reallocations transfer ownership; failed ones retain the original pointer.
    unsafe {
        let pointer = allocator.alloc_zeroed(initial);
        assert!(!pointer.is_null());
        assert_eq!(pointer as usize % 64, 0);
        for i in 0..32 { assert_eq!(*pointer.add(i), 0); *pointer.add(i) = i as u8; }
        let failed = allocator.realloc(pointer, initial, 256);
        assert!(failed.is_null());
        assert_eq!(allocator.frees.get(), 0);
        for i in 0..32 { assert_eq!(*pointer.add(i), i as u8); }
        let grown = allocator.realloc(pointer, initial, 96);
        assert!(!grown.is_null());
        assert_eq!(grown as usize % 64, 0);
        for i in 0..32 { assert_eq!(*grown.add(i), i as u8); }
        let shrunk = allocator.realloc(grown, Layout::from_size_align(96, 64).unwrap(), 8);
        assert!(!shrunk.is_null());
        for i in 0..8 { assert_eq!(*shrunk.add(i), i as u8); }
        allocator.dealloc(shrunk, Layout::from_size_align(8, 64).unwrap());
        assert!(allocator.alloc_zeroed(Layout::from_size_align(256, 64).unwrap()).is_null());
    }
    assert_eq!(allocator.frees.get(), 3);
    println!("freed {} blocks", allocator.frees.get());
}
