use std::mem::offset_of;
#[repr(C)]
struct Inner { tag: u8, value: u64 }
#[repr(C)]
struct Outer<T> { prefix: u8, payload: T, suffix: u16 }
#[repr(C, packed)]
struct Packed { tag: u8, value: u64 }
struct Named { r#type: u32 }
union Storage { small: u8, wide: u64 }
const VALUE: usize = offset_of!(Outer<Inner>, payload.value);
const PACKED: usize = offset_of!(Packed, value);
const fn generic<T>() -> usize { offset_of!(Outer<T>, payload) }
fn main() {
    assert_eq!(VALUE, 16);
    assert_eq!(offset_of!(Named, r#type,), 0);
    let _ = offset_of!(((u8,u64),), 0.1);
    assert_eq!(PACKED, 1);
    assert_eq!(offset_of!(Storage, wide), 0);
    assert_eq!(generic::<u16>(), 2);
    assert_eq!(generic::<u64>(), 8);
    assert_eq!(offset_of!(Outer<(u32,u8)>, payload.0), 4);
    println!("{} {} {} {}", VALUE, PACKED, generic::<u64>(), offset_of!(Outer<Inner>, suffix));
}
