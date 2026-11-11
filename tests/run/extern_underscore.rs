#![no_std]
#[macro_use]
extern crate std as _;
extern crate alloc;
extern crate alloc as _;
extern crate self as _;
fn main() {
    let mut values = alloc::vec![3u8, 5, 7];
    values.push(11);
    assert_eq!(values.len(), 4);
    println!("{} {}", values[0], values[3]);
}
