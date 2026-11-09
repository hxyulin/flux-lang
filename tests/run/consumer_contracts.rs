use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU32, Ordering};

const fn cell_pointer<T>(cell: &UnsafeCell<T>) -> *mut T {
    cell.get()
}
fn load(value: *mut u32) -> u32 {
    unsafe {
        let atomic: &AtomicU32 = &*(value as *const _ as *const _);
        atomic.load(Ordering::Relaxed)
    }
}
fn borrowed<'a>(value: &'a u32) -> &'a u32 {
    (|| {
        let value: &'a u32 = value;
        value
    })()
}
fn byte_pointer(value: *mut u32) -> *const u8 {
    value as *const _
}
fn main() {
    let cell = UnsafeCell::new(42u32);
    let pointer = cell_pointer(&cell);
    println!("{} {} {}", load(pointer), borrowed(&7), byte_pointer(pointer) == pointer.cast::<u8>());
}
