#![allow(unpredictable_function_pointer_comparisons)]

type Callback = unsafe extern "C-unwind" fn(u32) -> u32;

unsafe extern "C-unwind" fn identity(value: u32) -> u32 {
    value
}

fn increment(value: &u32) -> u32 {
    *value + 1
}

#[derive(PartialEq)]
struct Slot {
    callback: Callback,
}

fn main() {
    let callback: Callback = identity;
    let copied = callback;
    assert!(copied == callback);
    assert!(copied <= callback);
    assert!(Slot { callback } == Slot { callback: copied });
    assert!(std::ptr::fn_addr_eq(callback, copied));
    let other: fn(&u32) -> u32 = increment;
    assert_eq!(
        std::ptr::fn_addr_eq(callback, other),
        std::ptr::addr_eq(callback as *const (), other as *const ())
    );
    println!("same callback");
}
