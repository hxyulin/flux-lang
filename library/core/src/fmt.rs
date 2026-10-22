//! Formatting, as far as flux has it: the `Debug` trait, which
//! `#[derive(Debug)]` implements, and an opaque `Formatter`. Nothing
//! writes through a `Formatter` yet (there are no `format!` or `{:?}`
//! without macros), so implementations only need to type-check.

use crate::result::Result::Ok;

/// What formatting may fail with.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Debug)]
pub struct Error;

/// What formatting gives.
pub type Result = crate::result::Result<(), Error>;

/// Where formatted text goes, and how. Only `core` makes one.
pub struct Formatter<'a> {
    _out: &'a mut (),
}

impl Formatter<'_> {
    /// Writes `data`, which flux's formatters never show.
    pub fn write_str(&mut self, _data: &str) -> Result {
        Ok(())
    }

    /// Writes `args` (`write!(f, ..)`), which flux's formatters never
    /// show.
    pub fn write_fmt(&mut self, _args: Arguments<'_>) -> Result {
        Ok(())
    }
}

/// What `format_args!` makes: the literal pieces of its format string.
/// flux cannot format the arguments yet, so it does not keep them.
#[derive(Clone, Copy)]
pub struct Arguments<'a> {
    pieces: &'a [&'static str],
}

impl<'a> Arguments<'a> {
    #[doc(hidden)]
    pub const fn new_const(pieces: &'a [&'static str]) -> Arguments<'a> {
        Arguments { pieces }
    }
}

impl Display for Arguments<'_> {
    fn fmt(&self, _f: &mut Formatter<'_>) -> Result {
        Ok(())
    }
}

impl Debug for Arguments<'_> {
    fn fmt(&self, _f: &mut Formatter<'_>) -> Result {
        Ok(())
    }
}

/// Types that can be shown to users, as `{}` would. A failing bound
/// reads as rustc's.
#[diagnostic::on_unimplemented(message = "`{Self}` doesn't implement `std::fmt::Display`")]
pub trait Display {
    /// Writes the value to `f`.
    fn fmt(&self, f: &mut Formatter<'_>) -> Result;
}

/// `{:x}`
pub trait LowerHex {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result;
}

/// `{:X}`
pub trait UpperHex {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result;
}

/// `{:o}`
pub trait Octal {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result;
}

/// `{:b}`
pub trait Binary {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result;
}

/// `{:e}`
pub trait LowerExp {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result;
}

/// `{:E}`
pub trait UpperExp {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result;
}

/// `{:p}`
pub trait Pointer {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result;
}

/// Implements formatting traits for types, writing nothing: nothing
/// formats yet.
macro_rules! nothing {
    ($tr:ident for $($ty:ty),*) => {
        $(
            impl $tr for $ty {
                fn fmt(&self, _f: &mut Formatter<'_>) -> Result {
                    Ok(())
                }
            }
        )*
    };
}

nothing!(Display for i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, bool, char, str);
nothing!(LowerHex for i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);
nothing!(UpperHex for i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);
nothing!(Octal for i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);
nothing!(Binary for i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);
nothing!(LowerExp for i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);
nothing!(UpperExp for i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);

impl<T: Display> Display for &T {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        Display::fmt(*self, f)
    }
}

impl<T: Display> Display for &mut T {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        Display::fmt(&**self, f)
    }
}

impl<T> Pointer for &T {
    fn fmt(&self, _f: &mut Formatter<'_>) -> Result {
        Ok(())
    }
}

impl<T> Pointer for &mut T {
    fn fmt(&self, _f: &mut Formatter<'_>) -> Result {
        Ok(())
    }
}

impl<T> Pointer for *const T {
    fn fmt(&self, _f: &mut Formatter<'_>) -> Result {
        Ok(())
    }
}

impl<T> Pointer for *mut T {
    fn fmt(&self, _f: &mut Formatter<'_>) -> Result {
        Ok(())
    }
}

/// What `format_args!` checks of its arguments: that each implements the
/// trait its placeholder names.
#[doc(hidden)]
pub mod rt {
    use super::*;

    pub fn display<T: Display>(_: &T) {}
    pub fn debug<T: Debug>(_: &T) {}
    pub fn lower_hex<T: LowerHex>(_: &T) {}
    pub fn upper_hex<T: UpperHex>(_: &T) {}
    pub fn octal<T: Octal>(_: &T) {}
    pub fn binary<T: Binary>(_: &T) {}
    pub fn lower_exp<T: LowerExp>(_: &T) {}
    pub fn upper_exp<T: UpperExp>(_: &T) {}
    pub fn pointer<T: Pointer>(_: &T) {}
    pub fn count(_: &usize) {}
}

/// Types that can be shown for debugging, as `{:?}` would. A failing
/// bound reads "`T` doesn't implement `Debug`", as in rustc.
#[lang = "Debug"]
#[diagnostic::on_unimplemented(message = "`{Self}` doesn't implement `{This}`")]
pub trait Debug {
    /// Writes the value to `f`.
    fn fmt(&self, f: &mut Formatter<'_>) -> Result;
}

// Written out for each type, since flux has no macros.
impl Debug for i8 { fn fmt(&self, _f: &mut Formatter<'_>) -> Result { Ok(()) } }
impl Debug for i16 { fn fmt(&self, _f: &mut Formatter<'_>) -> Result { Ok(()) } }
impl Debug for i32 { fn fmt(&self, _f: &mut Formatter<'_>) -> Result { Ok(()) } }
impl Debug for i64 { fn fmt(&self, _f: &mut Formatter<'_>) -> Result { Ok(()) } }
impl Debug for i128 { fn fmt(&self, _f: &mut Formatter<'_>) -> Result { Ok(()) } }
impl Debug for isize { fn fmt(&self, _f: &mut Formatter<'_>) -> Result { Ok(()) } }
impl Debug for u8 { fn fmt(&self, _f: &mut Formatter<'_>) -> Result { Ok(()) } }
impl Debug for u16 { fn fmt(&self, _f: &mut Formatter<'_>) -> Result { Ok(()) } }
impl Debug for u32 { fn fmt(&self, _f: &mut Formatter<'_>) -> Result { Ok(()) } }
impl Debug for u64 { fn fmt(&self, _f: &mut Formatter<'_>) -> Result { Ok(()) } }
impl Debug for u128 { fn fmt(&self, _f: &mut Formatter<'_>) -> Result { Ok(()) } }
impl Debug for usize { fn fmt(&self, _f: &mut Formatter<'_>) -> Result { Ok(()) } }
impl Debug for bool { fn fmt(&self, _f: &mut Formatter<'_>) -> Result { Ok(()) } }
impl Debug for () { fn fmt(&self, _f: &mut Formatter<'_>) -> Result { Ok(()) } }
impl Debug for char { fn fmt(&self, _f: &mut Formatter<'_>) -> Result { Ok(()) } }
impl Debug for str { fn fmt(&self, _f: &mut Formatter<'_>) -> Result { Ok(()) } }
impl<T> Debug for *const T { fn fmt(&self, _f: &mut Formatter<'_>) -> Result { Ok(()) } }
impl<T> Debug for *mut T { fn fmt(&self, _f: &mut Formatter<'_>) -> Result { Ok(()) } }

impl<T: Debug> Debug for &T {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        Debug::fmt(*self, f)
    }
}

impl<T: Debug> Debug for &mut T {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        Debug::fmt(&**self, f)
    }
}

/// A slice, each element in turn. Arrays' is built into the compiler,
/// as a call of this one.
impl<T: Debug> Debug for [T] {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        for element in self {
            Debug::fmt(element, f)?;
        }
        Ok(())
    }
}

// Tuples of up to twelve elements, each in turn.
impl<A: Debug> Debug for (A,) { fn fmt(&self, f: &mut Formatter<'_>) -> Result { self.0.fmt(f) } }
impl<A: Debug, B: Debug> Debug for (A, B) { fn fmt(&self, f: &mut Formatter<'_>) -> Result { self.0.fmt(f)?; self.1.fmt(f) } }
impl<A: Debug, B: Debug, C: Debug> Debug for (A, B, C) { fn fmt(&self, f: &mut Formatter<'_>) -> Result { self.0.fmt(f)?; self.1.fmt(f)?; self.2.fmt(f) } }
impl<A: Debug, B: Debug, C: Debug, D: Debug> Debug for (A, B, C, D) { fn fmt(&self, f: &mut Formatter<'_>) -> Result { self.0.fmt(f)?; self.1.fmt(f)?; self.2.fmt(f)?; self.3.fmt(f) } }
impl<A: Debug, B: Debug, C: Debug, D: Debug, E: Debug> Debug for (A, B, C, D, E) { fn fmt(&self, f: &mut Formatter<'_>) -> Result { self.0.fmt(f)?; self.1.fmt(f)?; self.2.fmt(f)?; self.3.fmt(f)?; self.4.fmt(f) } }
impl<A: Debug, B: Debug, C: Debug, D: Debug, E: Debug, F: Debug> Debug for (A, B, C, D, E, F) { fn fmt(&self, f: &mut Formatter<'_>) -> Result { self.0.fmt(f)?; self.1.fmt(f)?; self.2.fmt(f)?; self.3.fmt(f)?; self.4.fmt(f)?; self.5.fmt(f) } }
impl<A: Debug, B: Debug, C: Debug, D: Debug, E: Debug, F: Debug, G: Debug> Debug for (A, B, C, D, E, F, G) { fn fmt(&self, f: &mut Formatter<'_>) -> Result { self.0.fmt(f)?; self.1.fmt(f)?; self.2.fmt(f)?; self.3.fmt(f)?; self.4.fmt(f)?; self.5.fmt(f)?; self.6.fmt(f) } }
impl<A: Debug, B: Debug, C: Debug, D: Debug, E: Debug, F: Debug, G: Debug, H: Debug> Debug for (A, B, C, D, E, F, G, H) { fn fmt(&self, f: &mut Formatter<'_>) -> Result { self.0.fmt(f)?; self.1.fmt(f)?; self.2.fmt(f)?; self.3.fmt(f)?; self.4.fmt(f)?; self.5.fmt(f)?; self.6.fmt(f)?; self.7.fmt(f) } }
impl<A: Debug, B: Debug, C: Debug, D: Debug, E: Debug, F: Debug, G: Debug, H: Debug, I: Debug> Debug for (A, B, C, D, E, F, G, H, I) { fn fmt(&self, f: &mut Formatter<'_>) -> Result { self.0.fmt(f)?; self.1.fmt(f)?; self.2.fmt(f)?; self.3.fmt(f)?; self.4.fmt(f)?; self.5.fmt(f)?; self.6.fmt(f)?; self.7.fmt(f)?; self.8.fmt(f) } }
impl<A: Debug, B: Debug, C: Debug, D: Debug, E: Debug, F: Debug, G: Debug, H: Debug, I: Debug, J: Debug> Debug for (A, B, C, D, E, F, G, H, I, J) { fn fmt(&self, f: &mut Formatter<'_>) -> Result { self.0.fmt(f)?; self.1.fmt(f)?; self.2.fmt(f)?; self.3.fmt(f)?; self.4.fmt(f)?; self.5.fmt(f)?; self.6.fmt(f)?; self.7.fmt(f)?; self.8.fmt(f)?; self.9.fmt(f) } }
impl<A: Debug, B: Debug, C: Debug, D: Debug, E: Debug, F: Debug, G: Debug, H: Debug, I: Debug, J: Debug, K: Debug> Debug for (A, B, C, D, E, F, G, H, I, J, K) { fn fmt(&self, f: &mut Formatter<'_>) -> Result { self.0.fmt(f)?; self.1.fmt(f)?; self.2.fmt(f)?; self.3.fmt(f)?; self.4.fmt(f)?; self.5.fmt(f)?; self.6.fmt(f)?; self.7.fmt(f)?; self.8.fmt(f)?; self.9.fmt(f)?; self.10.fmt(f) } }
impl<A: Debug, B: Debug, C: Debug, D: Debug, E: Debug, F: Debug, G: Debug, H: Debug, I: Debug, J: Debug, K: Debug, L: Debug> Debug for (A, B, C, D, E, F, G, H, I, J, K, L) { fn fmt(&self, f: &mut Formatter<'_>) -> Result { self.0.fmt(f)?; self.1.fmt(f)?; self.2.fmt(f)?; self.3.fmt(f)?; self.4.fmt(f)?; self.5.fmt(f)?; self.6.fmt(f)?; self.7.fmt(f)?; self.8.fmt(f)?; self.9.fmt(f)?; self.10.fmt(f)?; self.11.fmt(f) } }
