//! Deprecated integer modules remain usable by dependency sources.
#![allow(deprecated)]
use core::u8 as byte;
fn main() {
    assert_eq!(byte::MIN, u8::MIN);
    assert_eq!(byte::MAX, u8::MAX);
    assert_eq!(core::u32::MIN, u32::MIN);
    assert_eq!(core::u32::MAX, u32::MAX);
    assert_eq!(std::u8::MAX, core::u8::MAX);
    assert_eq!(std::u32::MAX, core::u32::MAX);
    println!("{} {} {} {}", byte::MIN, byte::MAX, core::u32::MIN, std::u32::MAX);
}
