//! Conversions between types.

/// The identity function: returns its argument unchanged.
pub const fn identity<T>(x: T) -> T {
    x
}

/// The error of a conversion that cannot fail: a type without values.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Infallible {}

/// Conversion from a `T`, which cannot fail. `?` converts the error of a
/// `Result` with it, to the error type of the function it returns from.
#[lang = "From"]
pub trait From<T>: Sized {
    fn from(value: T) -> Self;
}

/// Every type converts from itself.
impl<T> From<T> for T {
    fn from(t: T) -> T {
        t
    }
}

// The integer conversions that cannot lose information, and `bool` to
// each integer type, as in rustc.

impl From<u8> for u16 { fn from(small: u8) -> u16 { small as u16 } }
impl From<u8> for u32 { fn from(small: u8) -> u32 { small as u32 } }
impl From<u8> for u64 { fn from(small: u8) -> u64 { small as u64 } }
impl From<u8> for u128 { fn from(small: u8) -> u128 { small as u128 } }
impl From<u8> for usize { fn from(small: u8) -> usize { small as usize } }
impl From<u8> for i16 { fn from(small: u8) -> i16 { small as i16 } }
impl From<u8> for i32 { fn from(small: u8) -> i32 { small as i32 } }
impl From<u8> for i64 { fn from(small: u8) -> i64 { small as i64 } }
impl From<u8> for i128 { fn from(small: u8) -> i128 { small as i128 } }
impl From<u8> for isize { fn from(small: u8) -> isize { small as isize } }
impl From<u16> for u32 { fn from(small: u16) -> u32 { small as u32 } }
impl From<u16> for u64 { fn from(small: u16) -> u64 { small as u64 } }
impl From<u16> for u128 { fn from(small: u16) -> u128 { small as u128 } }
impl From<u16> for usize { fn from(small: u16) -> usize { small as usize } }
impl From<u16> for i32 { fn from(small: u16) -> i32 { small as i32 } }
impl From<u16> for i64 { fn from(small: u16) -> i64 { small as i64 } }
impl From<u16> for i128 { fn from(small: u16) -> i128 { small as i128 } }
impl From<u32> for u64 { fn from(small: u32) -> u64 { small as u64 } }
impl From<u32> for u128 { fn from(small: u32) -> u128 { small as u128 } }
impl From<u32> for i64 { fn from(small: u32) -> i64 { small as i64 } }
impl From<u32> for i128 { fn from(small: u32) -> i128 { small as i128 } }
impl From<u64> for u128 { fn from(small: u64) -> u128 { small as u128 } }
impl From<u64> for i128 { fn from(small: u64) -> i128 { small as i128 } }
impl From<i8> for i16 { fn from(small: i8) -> i16 { small as i16 } }
impl From<i8> for i32 { fn from(small: i8) -> i32 { small as i32 } }
impl From<i8> for i64 { fn from(small: i8) -> i64 { small as i64 } }
impl From<i8> for i128 { fn from(small: i8) -> i128 { small as i128 } }
impl From<i8> for isize { fn from(small: i8) -> isize { small as isize } }
impl From<i16> for i32 { fn from(small: i16) -> i32 { small as i32 } }
impl From<i16> for i64 { fn from(small: i16) -> i64 { small as i64 } }
impl From<i16> for i128 { fn from(small: i16) -> i128 { small as i128 } }
impl From<i16> for isize { fn from(small: i16) -> isize { small as isize } }
impl From<i32> for i64 { fn from(small: i32) -> i64 { small as i64 } }
impl From<i32> for i128 { fn from(small: i32) -> i128 { small as i128 } }
impl From<i64> for i128 { fn from(small: i64) -> i128 { small as i128 } }
impl From<bool> for i8 { fn from(b: bool) -> i8 { b as i8 } }
impl From<bool> for i16 { fn from(b: bool) -> i16 { b as i16 } }
impl From<bool> for i32 { fn from(b: bool) -> i32 { b as i32 } }
impl From<bool> for i64 { fn from(b: bool) -> i64 { b as i64 } }
impl From<bool> for i128 { fn from(b: bool) -> i128 { b as i128 } }
impl From<bool> for isize { fn from(b: bool) -> isize { b as isize } }
impl From<bool> for u8 { fn from(b: bool) -> u8 { b as u8 } }
impl From<bool> for u16 { fn from(b: bool) -> u16 { b as u16 } }
impl From<bool> for u32 { fn from(b: bool) -> u32 { b as u32 } }
impl From<bool> for u64 { fn from(b: bool) -> u64 { b as u64 } }
impl From<bool> for u128 { fn from(b: bool) -> u128 { b as u128 } }
impl From<bool> for usize { fn from(b: bool) -> usize { b as usize } }
