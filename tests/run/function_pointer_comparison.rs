#![allow(unpredictable_function_pointer_comparisons)]

type Callback = unsafe extern "C-unwind" fn(u32) -> u32;

unsafe extern "C-unwind" fn identity(value: u32) -> u32 {
    value
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
    println!("same callback");
}
