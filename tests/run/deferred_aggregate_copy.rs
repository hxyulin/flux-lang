type Bytes = [u8; 128];

use std::sync::atomic::{AtomicUsize, Ordering};

static DROPS: AtomicUsize = AtomicUsize::new(0);

struct Owned(Bytes);

impl Drop for Owned {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

#[inline(never)]
unsafe fn change(input: *mut Bytes) {
    // SAFETY: callers pass an initialized, writable array.
    unsafe { (*input)[0] = 241 };
}

#[inline(never)]
pub unsafe fn transfer(output: *mut Bytes, value: Bytes, input: *mut Bytes, change_input: bool) {
    if change_input {
        // SAFETY: the caller permits the input array to be changed here.
        unsafe { change(input) };
    }
    // SAFETY: output points to valid storage for a complete array.
    unsafe { std::ptr::write(output, value) };
}

#[inline(never)]
pub unsafe fn transfer_and_read(output: *mut Bytes, value: Bytes, input: *mut Bytes) -> u8 {
    // SAFETY: the caller provides valid input and output storage.
    unsafe {
        change(input);
        std::ptr::write(output, value);
    }
    value[0]
}

#[inline(never)]
pub unsafe fn transfer_two_changes(
    output: *mut Bytes,
    value: Bytes,
    input: *mut Bytes,
    change_input: bool,
) {
    if change_input {
        // SAFETY: the caller permits the input array to be changed here.
        unsafe {
            change(input);
            change(input);
        }
    }
    // SAFETY: output points to valid storage for a complete array.
    unsafe { std::ptr::write(output, value) };
}

fn main() {
    let old = std::array::from_fn(|i| (i + 19) as u8);
    let mut input = old;
    let mut output = [0; 128];
    let source = &raw mut input;
    // SAFETY: input and output are initialized, separate writable arrays.
    unsafe { transfer(&raw mut output, *source, source, true) };
    assert_eq!(input[0], 241);
    assert_eq!(output, old);

    let mut storage = [0; 160];
    storage[..128].copy_from_slice(&old);
    let source = storage.as_mut_ptr().cast::<Bytes>();
    // SAFETY: both ranges are within storage. The value is passed by value,
    // so writing the overlapping destination cannot change the input value.
    unsafe { transfer(source.cast::<u8>().add(7).cast(), *source, source, false) };
    assert_eq!(&storage[7..135], &old[..]);

    let mut input = old;
    let source = &raw mut input;
    // SAFETY: these arguments point to initialized, separate writable arrays.
    let seen = unsafe { transfer_and_read(&raw mut output, *source, source) };
    assert_eq!(seen, 19);
    assert_eq!(output, old);

    let mut input = old;
    let source = &raw mut input;
    // SAFETY: these arguments point to initialized, separate writable arrays.
    unsafe { transfer_two_changes(&raw mut output, *source, source, true) };
    assert_eq!(input[0], 241);
    assert_eq!(output, old);

    let mut values = Vec::with_capacity(1);
    values.push(old);
    for i in 1..17 {
        let mut next = old;
        next[0] = i;
        values.push(next);
    }
    for (i, value) in values.iter().enumerate().skip(1) {
        assert_eq!(value[0], i as u8);
        assert_eq!(&value[1..], &old[1..]);
    }

    let mut owned = Vec::with_capacity(1);
    for _ in 0..8 {
        owned.push(Owned(old));
    }
    assert!(owned.iter().all(|item| item.0 == old));
    assert_eq!(DROPS.load(Ordering::Relaxed), 0);
    drop(owned);
    assert_eq!(DROPS.load(Ordering::Relaxed), 8);
    println!("deferred aggregate copies verified");
}
