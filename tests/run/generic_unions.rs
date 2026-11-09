use core::mem::{ManuallyDrop, align_of, size_of};
#[repr(C)]
union Slot<T> {
    value: ManuallyDrop<T>,
    empty: (),
}
union CopySlot<T: Copy> {
    value: T,
}
union Bytes<const N: usize> {
    bytes: [u8; N],
    word: u32,
}
fn recover<T>(value: T) -> T {
    let slot = Slot { value: ManuallyDrop::new(value) };
    ManuallyDrop::into_inner(unsafe { slot.value })
}
fn main() {
    let value = 42;
    let slot = CopySlot { value: &value };
    println!("{} {}", unsafe { *slot.value }, recover(String::from("hello")));
    let bytes = Bytes { bytes: [7u8; 8] };
    println!("{} {} {}", unsafe { bytes.bytes[7] }, size_of::<Bytes<8>>(), align_of::<Bytes<8>>());
    let mut slot = CopySlot { value: 3u64 };
    slot.value = 9;
    println!("{}", unsafe { slot.value });
}
