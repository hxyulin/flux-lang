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
