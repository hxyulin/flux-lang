extern crate alloc as storage;
extern crate core as rust_core;

use rust_core::error::Error;
use rust_core::num::NonZeroU64;

#[repr(transparent)]
struct Tag(u8);
#[repr(transparent)]
struct Wrapped<T>(T);

#[derive(Debug)]
struct Failure;
impl rust_core::fmt::Display for Failure {
    fn fmt(&self, f: &mut rust_core::fmt::Formatter<'_>) -> rust_core::fmt::Result {
        f.write_str("failure")
    }
}
impl Error for Failure {}

fn check_nonzero(value: u64) {
    let nonzero = NonZeroU64::new(value);
    match nonzero {
        Some(value) => println!("nz {} {} {}", value, value.leading_zeros(), value.trailing_zeros()),
        None => println!("zero"),
    }
}

fn main() {
    let mut values = [String::from("one"), String::from("two"), String::from("three"), String::from("four")];
    unsafe { rust_core::ptr::swap_nonoverlapping(values.as_mut_ptr(), values.as_mut_ptr().add(2), 2); }
    println!("{} {} {} {}", values[0], values[1], values[2], values[3]);
    let empty = rust_core::ptr::slice_from_raw_parts_mut::<u64>(rust_core::ptr::null_mut(), 37);
    println!("raw {} {}", empty.len(), empty as *mut u64 as usize);
    let shared = rust_core::ptr::slice_from_raw_parts::<u64>(rust_core::ptr::null(), usize::MAX);
    println!("raw-max {} {}", shared.len(), shared as *const u64 as usize);
    let layout = storage::alloc::Layout::from_size_align(96, 64).unwrap();
    unsafe {
        let ptr = storage::alloc::alloc(layout);
        assert!(!ptr.is_null());
        assert_eq!(ptr as usize % 64, 0);
        let slice = rust_core::ptr::slice_from_raw_parts_mut(ptr, 96);
        (*slice)[95] = 73;
        println!("alloc {} {}", slice.len(), (*slice)[95]);
        storage::alloc::dealloc(ptr, layout);
    }
    let tag = Tag(0x80);
    println!("tag {} {} {}", tag.0, rust_core::mem::size_of::<Tag>(), rust_core::mem::align_of::<Tag>());
    let boxed = Wrapped(Box::new(41u64));
    println!("wrapped {} {}", *boxed.0, rust_core::mem::size_of::<Wrapped<Box<u64>>>());
    println!("error {} {}", Failure, Failure.source().is_none());
    for value in [0, 1, 128, u64::MAX, 1u64 << 63] { check_nonzero(value); }
    let value = unsafe { NonZeroU64::new_unchecked(256) };
    let values = [None, Some(value), NonZeroU64::new(u64::MAX)];
    println!("niche {} {:?}", rust_core::mem::size_of::<Option<NonZeroU64>>(), values);
}
