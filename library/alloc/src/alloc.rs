//! Allocating memory through C's `malloc`, `realloc` and `free`, as
//! Rust's `System` allocator does on macOS: `posix_memalign` for an
//! alignment past `malloc`'s 16 bytes or past the size.

pub use core::alloc::{Layout, LayoutError};

mod libc {
    unsafe extern "C" {
        pub fn malloc(size: usize) -> *mut u8;
        pub fn calloc(count: usize, size: usize) -> *mut u8;
        pub fn realloc(ptr: *mut u8, size: usize) -> *mut u8;
        pub fn free(ptr: *mut u8);
        pub fn posix_memalign(memptr: *mut *mut u8, align: usize, size: usize) -> i32;
        pub fn abort() -> !;
    }
}

/// The alignment `malloc` gives every block on arm64 macOS.
const MIN_ALIGN: usize = 16;

unsafe fn aligned_malloc(layout: &Layout) -> *mut u8 {
    let mut out: *mut u8 = core::ptr::null_mut();
    let align = if layout.align() < 8 { 8 } else { layout.align() };
    let ret = unsafe { libc::posix_memalign(&mut out, align, layout.size()) };
    if ret != 0 { core::ptr::null_mut() } else { out }
}

/// Memory for `layout`, uninitialized; null if there is none.
pub unsafe fn alloc(layout: Layout) -> *mut u8 {
    if layout.align() <= MIN_ALIGN && layout.align() <= layout.size() {
        unsafe { libc::malloc(layout.size()) }
    } else {
        unsafe { aligned_malloc(&layout) }
    }
}

/// Memory for `layout`, its bytes zero; null if there is none.
pub unsafe fn alloc_zeroed(layout: Layout) -> *mut u8 {
    if layout.align() <= MIN_ALIGN && layout.align() <= layout.size() {
        unsafe { libc::calloc(layout.size(), 1) }
    } else {
        let ptr = unsafe { alloc(layout) };
        if !ptr.is_null() {
            unsafe { core::ptr::write_bytes(ptr, 0, layout.size()) };
        }
        ptr
    }
}

/// Frees `ptr`, which `alloc` gave for `layout`.
pub unsafe fn dealloc(ptr: *mut u8, layout: Layout) {
    let _ = layout;
    unsafe { libc::free(ptr) }
}

/// `ptr`'s memory, which `alloc` gave for `layout`, grown or shrunk to
/// `new_size` bytes, its contents kept; null if there is none.
pub unsafe fn realloc(ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
    if layout.align() <= MIN_ALIGN && layout.align() <= new_size {
        unsafe { libc::realloc(ptr, new_size) }
    } else {
        let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
        let new_ptr = unsafe { alloc(new_layout) };
        if !new_ptr.is_null() {
            let size = if layout.size() < new_size { layout.size() } else { new_size };
            unsafe {
                core::ptr::copy_nonoverlapping(ptr, new_ptr, size);
                dealloc(ptr, layout);
            }
        }
        new_ptr
    }
}

/// What failing to allocate does: abort, as rustc's default handler
/// does after printing its message.
pub fn handle_alloc_error(layout: Layout) -> ! {
    let _ = layout;
    unsafe { libc::abort() }
}
