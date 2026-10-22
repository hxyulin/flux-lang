//! Conversions between types.

use crate::num::{IntErrorKind, TryFromIntError};
use crate::option::Option::{self, None, Some};
use crate::result::Result::{self, Err, Ok};

/// The identity function: returns its argument unchanged.
pub const fn identity<T>(x: T) -> T {
    x
}

/// The error of a conversion that cannot fail: a type without values.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
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

/// Conversion into a `T`: what `From` gives, the other way round. Every
/// type converts into those that convert from it.
pub trait Into<T>: Sized {
    fn into(self) -> T;
}

impl<T, U: From<T>> Into<U> for T {
    fn into(self) -> U {
        U::from(self)
    }
}

/// Conversion from a `T` that may fail, with `Error`.
pub trait TryFrom<T>: Sized {
    /// Why a conversion failed.
    type Error;
    fn try_from(value: T) -> Result<Self, Self::Error>;
}

/// Conversion into a `T` that may fail: what `TryFrom` gives, the other
/// way round.
pub trait TryInto<T>: Sized {
    /// Why a conversion failed.
    type Error;
    fn try_into(self) -> Result<T, Self::Error>;
}

impl<T, U: TryFrom<T>> TryInto<U> for T {
    type Error = U::Error;

    fn try_into(self) -> Result<U, U::Error> {
        U::try_from(self)
    }
}

/// A conversion that cannot fail cannot fail as a `TryFrom` either.
impl<T, U: Into<T>> TryFrom<U> for T {
    type Error = Infallible;

    fn try_from(value: U) -> Result<T, Infallible> {
        Ok(U::into(value))
    }
}

/// A cheap conversion of a reference to one to a `T`.
pub trait AsRef<T: ?Sized> {
    fn as_ref(&self) -> &T;
}

/// A cheap conversion of a mutable reference to one to a `T`.
pub trait AsMut<T: ?Sized> {
    fn as_mut(&mut self) -> &mut T;
}

impl<T: ?Sized, U: ?Sized> AsRef<U> for &T
where
    T: AsRef<U>,
{
    fn as_ref(&self) -> &U {
        <T as AsRef<U>>::as_ref(*self)
    }
}

impl<T: ?Sized, U: ?Sized> AsRef<U> for &mut T
where
    T: AsRef<U>,
{
    fn as_ref(&self) -> &U {
        <T as AsRef<U>>::as_ref(*self)
    }
}

impl<T: ?Sized, U: ?Sized> AsMut<U> for &mut T
where
    T: AsMut<U>,
{
    fn as_mut(&mut self) -> &mut U {
        <T as AsMut<U>>::as_mut(*self)
    }
}

impl<T> AsRef<[T]> for [T] {
    fn as_ref(&self) -> &[T] {
        self
    }
}

impl<T> AsMut<[T]> for [T] {
    fn as_mut(&mut self) -> &mut [T] {
        self
    }
}

impl<T, const N: usize> AsRef<[T]> for [T; N] {
    fn as_ref(&self) -> &[T] {
        self
    }
}

impl<T, const N: usize> AsMut<[T]> for [T; N] {
    fn as_mut(&mut self) -> &mut [T] {
        self
    }
}

impl AsRef<str> for str {
    fn as_ref(&self) -> &str {
        self
    }
}

impl AsMut<str> for str {
    fn as_mut(&mut self) -> &mut str {
        self
    }
}

impl AsRef<[u8]> for str {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

/// `Some(t)`.
impl<T> From<T> for Option<T> {
    fn from(val: T) -> Option<T> {
        Some(val)
    }
}

// Characters from `u32`s and to `u8`s and `u16`s, which may fail.

impl TryFrom<u32> for char {
    type Error = crate::char::CharTryFromError;

    fn try_from(i: u32) -> Result<char, crate::char::CharTryFromError> {
        match crate::char::from_u32(i) {
            Some(c) => Ok(c),
            None => Err(crate::char::CharTryFromError(())),
        }
    }
}

impl TryFrom<char> for u8 {
    type Error = crate::char::TryFromCharError;

    fn try_from(c: char) -> Result<u8, crate::char::TryFromCharError> {
        let code = c as u32;
        if code <= 0xFF { Ok(code as u8) } else { Err(crate::char::TryFromCharError(())) }
    }
}

impl TryFrom<char> for u16 {
    type Error = crate::char::TryFromCharError;

    fn try_from(c: char) -> Result<u16, crate::char::TryFromCharError> {
        let code = c as u32;
        if code <= 0xFFFF { Ok(code as u16) } else { Err(crate::char::TryFromCharError(())) }
    }
}

// The integer conversions that may lose information, as rustc has them:
// the value if the target type holds it, else `TryFromIntError`. Those
// that cannot lose any are `From`'s, and so `TryFrom`'s with
// `Infallible`.

impl TryFrom<u8> for i8 { type Error = TryFromIntError; fn try_from(u: u8) -> Result<i8, TryFromIntError> { let v = u as i8; if v >= 0 && v as u8 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u16> for u8 { type Error = TryFromIntError; fn try_from(u: u16) -> Result<u8, TryFromIntError> { let v = u as u8; if v as u16 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u16> for i8 { type Error = TryFromIntError; fn try_from(u: u16) -> Result<i8, TryFromIntError> { let v = u as i8; if v >= 0 && v as u16 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u16> for i16 { type Error = TryFromIntError; fn try_from(u: u16) -> Result<i16, TryFromIntError> { let v = u as i16; if v >= 0 && v as u16 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u16> for isize { type Error = TryFromIntError; fn try_from(u: u16) -> Result<isize, TryFromIntError> { let v = u as isize; if v >= 0 && v as u16 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u32> for u8 { type Error = TryFromIntError; fn try_from(u: u32) -> Result<u8, TryFromIntError> { let v = u as u8; if v as u32 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u32> for u16 { type Error = TryFromIntError; fn try_from(u: u32) -> Result<u16, TryFromIntError> { let v = u as u16; if v as u32 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u32> for usize { type Error = TryFromIntError; fn try_from(u: u32) -> Result<usize, TryFromIntError> { let v = u as usize; if v as u32 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u32> for i8 { type Error = TryFromIntError; fn try_from(u: u32) -> Result<i8, TryFromIntError> { let v = u as i8; if v >= 0 && v as u32 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u32> for i16 { type Error = TryFromIntError; fn try_from(u: u32) -> Result<i16, TryFromIntError> { let v = u as i16; if v >= 0 && v as u32 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u32> for i32 { type Error = TryFromIntError; fn try_from(u: u32) -> Result<i32, TryFromIntError> { let v = u as i32; if v >= 0 && v as u32 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u32> for isize { type Error = TryFromIntError; fn try_from(u: u32) -> Result<isize, TryFromIntError> { let v = u as isize; if v >= 0 && v as u32 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u64> for u8 { type Error = TryFromIntError; fn try_from(u: u64) -> Result<u8, TryFromIntError> { let v = u as u8; if v as u64 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u64> for u16 { type Error = TryFromIntError; fn try_from(u: u64) -> Result<u16, TryFromIntError> { let v = u as u16; if v as u64 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u64> for u32 { type Error = TryFromIntError; fn try_from(u: u64) -> Result<u32, TryFromIntError> { let v = u as u32; if v as u64 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u64> for usize { type Error = TryFromIntError; fn try_from(u: u64) -> Result<usize, TryFromIntError> { let v = u as usize; if v as u64 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u64> for i8 { type Error = TryFromIntError; fn try_from(u: u64) -> Result<i8, TryFromIntError> { let v = u as i8; if v >= 0 && v as u64 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u64> for i16 { type Error = TryFromIntError; fn try_from(u: u64) -> Result<i16, TryFromIntError> { let v = u as i16; if v >= 0 && v as u64 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u64> for i32 { type Error = TryFromIntError; fn try_from(u: u64) -> Result<i32, TryFromIntError> { let v = u as i32; if v >= 0 && v as u64 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u64> for i64 { type Error = TryFromIntError; fn try_from(u: u64) -> Result<i64, TryFromIntError> { let v = u as i64; if v >= 0 && v as u64 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u64> for isize { type Error = TryFromIntError; fn try_from(u: u64) -> Result<isize, TryFromIntError> { let v = u as isize; if v >= 0 && v as u64 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u128> for u8 { type Error = TryFromIntError; fn try_from(u: u128) -> Result<u8, TryFromIntError> { let v = u as u8; if v as u128 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u128> for u16 { type Error = TryFromIntError; fn try_from(u: u128) -> Result<u16, TryFromIntError> { let v = u as u16; if v as u128 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u128> for u32 { type Error = TryFromIntError; fn try_from(u: u128) -> Result<u32, TryFromIntError> { let v = u as u32; if v as u128 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u128> for u64 { type Error = TryFromIntError; fn try_from(u: u128) -> Result<u64, TryFromIntError> { let v = u as u64; if v as u128 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u128> for usize { type Error = TryFromIntError; fn try_from(u: u128) -> Result<usize, TryFromIntError> { let v = u as usize; if v as u128 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u128> for i8 { type Error = TryFromIntError; fn try_from(u: u128) -> Result<i8, TryFromIntError> { let v = u as i8; if v >= 0 && v as u128 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u128> for i16 { type Error = TryFromIntError; fn try_from(u: u128) -> Result<i16, TryFromIntError> { let v = u as i16; if v >= 0 && v as u128 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u128> for i32 { type Error = TryFromIntError; fn try_from(u: u128) -> Result<i32, TryFromIntError> { let v = u as i32; if v >= 0 && v as u128 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u128> for i64 { type Error = TryFromIntError; fn try_from(u: u128) -> Result<i64, TryFromIntError> { let v = u as i64; if v >= 0 && v as u128 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u128> for i128 { type Error = TryFromIntError; fn try_from(u: u128) -> Result<i128, TryFromIntError> { let v = u as i128; if v >= 0 && v as u128 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<u128> for isize { type Error = TryFromIntError; fn try_from(u: u128) -> Result<isize, TryFromIntError> { let v = u as isize; if v >= 0 && v as u128 == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<usize> for u8 { type Error = TryFromIntError; fn try_from(u: usize) -> Result<u8, TryFromIntError> { let v = u as u8; if v as usize == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<usize> for u16 { type Error = TryFromIntError; fn try_from(u: usize) -> Result<u16, TryFromIntError> { let v = u as u16; if v as usize == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<usize> for u32 { type Error = TryFromIntError; fn try_from(u: usize) -> Result<u32, TryFromIntError> { let v = u as u32; if v as usize == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<usize> for u64 { type Error = TryFromIntError; fn try_from(u: usize) -> Result<u64, TryFromIntError> { let v = u as u64; if v as usize == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<usize> for u128 { type Error = TryFromIntError; fn try_from(u: usize) -> Result<u128, TryFromIntError> { let v = u as u128; if v as usize == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<usize> for i8 { type Error = TryFromIntError; fn try_from(u: usize) -> Result<i8, TryFromIntError> { let v = u as i8; if v >= 0 && v as usize == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<usize> for i16 { type Error = TryFromIntError; fn try_from(u: usize) -> Result<i16, TryFromIntError> { let v = u as i16; if v >= 0 && v as usize == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<usize> for i32 { type Error = TryFromIntError; fn try_from(u: usize) -> Result<i32, TryFromIntError> { let v = u as i32; if v >= 0 && v as usize == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<usize> for i64 { type Error = TryFromIntError; fn try_from(u: usize) -> Result<i64, TryFromIntError> { let v = u as i64; if v >= 0 && v as usize == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<usize> for i128 { type Error = TryFromIntError; fn try_from(u: usize) -> Result<i128, TryFromIntError> { let v = u as i128; if v >= 0 && v as usize == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<usize> for isize { type Error = TryFromIntError; fn try_from(u: usize) -> Result<isize, TryFromIntError> { let v = u as isize; if v >= 0 && v as usize == u { Ok(v) } else { Err(TryFromIntError(IntErrorKind::PosOverflow)) } } }
impl TryFrom<i8> for u8 { type Error = TryFromIntError; fn try_from(u: i8) -> Result<u8, TryFromIntError> { let v = u as u8; if u >= 0 && v as i8 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i8> for u16 { type Error = TryFromIntError; fn try_from(u: i8) -> Result<u16, TryFromIntError> { let v = u as u16; if u >= 0 && v as i8 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i8> for u32 { type Error = TryFromIntError; fn try_from(u: i8) -> Result<u32, TryFromIntError> { let v = u as u32; if u >= 0 && v as i8 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i8> for u64 { type Error = TryFromIntError; fn try_from(u: i8) -> Result<u64, TryFromIntError> { let v = u as u64; if u >= 0 && v as i8 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i8> for u128 { type Error = TryFromIntError; fn try_from(u: i8) -> Result<u128, TryFromIntError> { let v = u as u128; if u >= 0 && v as i8 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i8> for usize { type Error = TryFromIntError; fn try_from(u: i8) -> Result<usize, TryFromIntError> { let v = u as usize; if u >= 0 && v as i8 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i16> for u8 { type Error = TryFromIntError; fn try_from(u: i16) -> Result<u8, TryFromIntError> { let v = u as u8; if u >= 0 && v as i16 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i16> for u16 { type Error = TryFromIntError; fn try_from(u: i16) -> Result<u16, TryFromIntError> { let v = u as u16; if u >= 0 && v as i16 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i16> for u32 { type Error = TryFromIntError; fn try_from(u: i16) -> Result<u32, TryFromIntError> { let v = u as u32; if u >= 0 && v as i16 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i16> for u64 { type Error = TryFromIntError; fn try_from(u: i16) -> Result<u64, TryFromIntError> { let v = u as u64; if u >= 0 && v as i16 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i16> for u128 { type Error = TryFromIntError; fn try_from(u: i16) -> Result<u128, TryFromIntError> { let v = u as u128; if u >= 0 && v as i16 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i16> for usize { type Error = TryFromIntError; fn try_from(u: i16) -> Result<usize, TryFromIntError> { let v = u as usize; if u >= 0 && v as i16 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i16> for i8 { type Error = TryFromIntError; fn try_from(u: i16) -> Result<i8, TryFromIntError> { let v = u as i8; if v as i16 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i32> for u8 { type Error = TryFromIntError; fn try_from(u: i32) -> Result<u8, TryFromIntError> { let v = u as u8; if u >= 0 && v as i32 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i32> for u16 { type Error = TryFromIntError; fn try_from(u: i32) -> Result<u16, TryFromIntError> { let v = u as u16; if u >= 0 && v as i32 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i32> for u32 { type Error = TryFromIntError; fn try_from(u: i32) -> Result<u32, TryFromIntError> { let v = u as u32; if u >= 0 && v as i32 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i32> for u64 { type Error = TryFromIntError; fn try_from(u: i32) -> Result<u64, TryFromIntError> { let v = u as u64; if u >= 0 && v as i32 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i32> for u128 { type Error = TryFromIntError; fn try_from(u: i32) -> Result<u128, TryFromIntError> { let v = u as u128; if u >= 0 && v as i32 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i32> for usize { type Error = TryFromIntError; fn try_from(u: i32) -> Result<usize, TryFromIntError> { let v = u as usize; if u >= 0 && v as i32 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i32> for i8 { type Error = TryFromIntError; fn try_from(u: i32) -> Result<i8, TryFromIntError> { let v = u as i8; if v as i32 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i32> for i16 { type Error = TryFromIntError; fn try_from(u: i32) -> Result<i16, TryFromIntError> { let v = u as i16; if v as i32 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i32> for isize { type Error = TryFromIntError; fn try_from(u: i32) -> Result<isize, TryFromIntError> { let v = u as isize; if v as i32 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i64> for u8 { type Error = TryFromIntError; fn try_from(u: i64) -> Result<u8, TryFromIntError> { let v = u as u8; if u >= 0 && v as i64 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i64> for u16 { type Error = TryFromIntError; fn try_from(u: i64) -> Result<u16, TryFromIntError> { let v = u as u16; if u >= 0 && v as i64 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i64> for u32 { type Error = TryFromIntError; fn try_from(u: i64) -> Result<u32, TryFromIntError> { let v = u as u32; if u >= 0 && v as i64 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i64> for u64 { type Error = TryFromIntError; fn try_from(u: i64) -> Result<u64, TryFromIntError> { let v = u as u64; if u >= 0 && v as i64 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i64> for u128 { type Error = TryFromIntError; fn try_from(u: i64) -> Result<u128, TryFromIntError> { let v = u as u128; if u >= 0 && v as i64 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i64> for usize { type Error = TryFromIntError; fn try_from(u: i64) -> Result<usize, TryFromIntError> { let v = u as usize; if u >= 0 && v as i64 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i64> for i8 { type Error = TryFromIntError; fn try_from(u: i64) -> Result<i8, TryFromIntError> { let v = u as i8; if v as i64 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i64> for i16 { type Error = TryFromIntError; fn try_from(u: i64) -> Result<i16, TryFromIntError> { let v = u as i16; if v as i64 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i64> for i32 { type Error = TryFromIntError; fn try_from(u: i64) -> Result<i32, TryFromIntError> { let v = u as i32; if v as i64 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i64> for isize { type Error = TryFromIntError; fn try_from(u: i64) -> Result<isize, TryFromIntError> { let v = u as isize; if v as i64 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i128> for u8 { type Error = TryFromIntError; fn try_from(u: i128) -> Result<u8, TryFromIntError> { let v = u as u8; if u >= 0 && v as i128 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i128> for u16 { type Error = TryFromIntError; fn try_from(u: i128) -> Result<u16, TryFromIntError> { let v = u as u16; if u >= 0 && v as i128 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i128> for u32 { type Error = TryFromIntError; fn try_from(u: i128) -> Result<u32, TryFromIntError> { let v = u as u32; if u >= 0 && v as i128 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i128> for u64 { type Error = TryFromIntError; fn try_from(u: i128) -> Result<u64, TryFromIntError> { let v = u as u64; if u >= 0 && v as i128 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i128> for u128 { type Error = TryFromIntError; fn try_from(u: i128) -> Result<u128, TryFromIntError> { let v = u as u128; if u >= 0 && v as i128 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i128> for usize { type Error = TryFromIntError; fn try_from(u: i128) -> Result<usize, TryFromIntError> { let v = u as usize; if u >= 0 && v as i128 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i128> for i8 { type Error = TryFromIntError; fn try_from(u: i128) -> Result<i8, TryFromIntError> { let v = u as i8; if v as i128 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i128> for i16 { type Error = TryFromIntError; fn try_from(u: i128) -> Result<i16, TryFromIntError> { let v = u as i16; if v as i128 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i128> for i32 { type Error = TryFromIntError; fn try_from(u: i128) -> Result<i32, TryFromIntError> { let v = u as i32; if v as i128 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i128> for i64 { type Error = TryFromIntError; fn try_from(u: i128) -> Result<i64, TryFromIntError> { let v = u as i64; if v as i128 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<i128> for isize { type Error = TryFromIntError; fn try_from(u: i128) -> Result<isize, TryFromIntError> { let v = u as isize; if v as i128 == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<isize> for u8 { type Error = TryFromIntError; fn try_from(u: isize) -> Result<u8, TryFromIntError> { let v = u as u8; if u >= 0 && v as isize == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<isize> for u16 { type Error = TryFromIntError; fn try_from(u: isize) -> Result<u16, TryFromIntError> { let v = u as u16; if u >= 0 && v as isize == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<isize> for u32 { type Error = TryFromIntError; fn try_from(u: isize) -> Result<u32, TryFromIntError> { let v = u as u32; if u >= 0 && v as isize == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<isize> for u64 { type Error = TryFromIntError; fn try_from(u: isize) -> Result<u64, TryFromIntError> { let v = u as u64; if u >= 0 && v as isize == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<isize> for u128 { type Error = TryFromIntError; fn try_from(u: isize) -> Result<u128, TryFromIntError> { let v = u as u128; if u >= 0 && v as isize == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<isize> for usize { type Error = TryFromIntError; fn try_from(u: isize) -> Result<usize, TryFromIntError> { let v = u as usize; if u >= 0 && v as isize == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<isize> for i8 { type Error = TryFromIntError; fn try_from(u: isize) -> Result<i8, TryFromIntError> { let v = u as i8; if v as isize == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<isize> for i16 { type Error = TryFromIntError; fn try_from(u: isize) -> Result<i16, TryFromIntError> { let v = u as i16; if v as isize == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<isize> for i32 { type Error = TryFromIntError; fn try_from(u: isize) -> Result<i32, TryFromIntError> { let v = u as i32; if v as isize == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<isize> for i64 { type Error = TryFromIntError; fn try_from(u: isize) -> Result<i64, TryFromIntError> { let v = u as i64; if v as isize == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
impl TryFrom<isize> for i128 { type Error = TryFromIntError; fn try_from(u: isize) -> Result<i128, TryFromIntError> { let v = u as i128; if v as isize == u { Ok(v) } else { Err(TryFromIntError(if u < 0 { IntErrorKind::NegOverflow } else { IntErrorKind::PosOverflow })) } } }
