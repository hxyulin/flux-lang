#![allow(improper_ctypes_definitions)]

extern "C" fn tail(value: *mut [u8]) -> *mut [u8] {
    let slice = unsafe { &mut *value };
    slice[1] += 7;
    core::ptr::slice_from_raw_parts_mut(slice.as_mut_ptr().wrapping_add(1), slice.len() - 1)
}
extern "C-unwind" fn text(value: *const str) -> *const str { value }
#[inline(never)]
fn indirect(callback: extern "C" fn(*mut [u8]) -> *mut [u8], value: *mut [u8]) -> *mut [u8] {
    callback(value)
}
fn main() {
    let mut values = [10,20,30,40];
    let pointer = core::ptr::slice_from_raw_parts_mut(values.as_mut_ptr(),values.len());
    let result = indirect(std::hint::black_box(tail),pointer);
    println!("{:?} {:?}",unsafe { &*result },values);
    let result = text("raw string" as *const str);
    println!("{}",unsafe { &*result });
}
