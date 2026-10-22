//! What `format_args!` builds an `Arguments` from: each value with the
//! function that formats it, and each placeholder's options.

use super::*;

/// A value a placeholder shows, as its address and the function that
/// writes it, or a width or precision (`{:1$}`, `{:.*}`).
#[derive(Clone, Copy)]
pub struct Argument<'a> {
    value: *const (),
    formatter: fn(*const (), &mut Formatter<'_>) -> Result,
    count: u16,
    _lifetime: PhantomData<&'a ()>,
}

/// Formats the `T` at `value` as `Display` does.
fn display_at<T: Display>(value: *const (), f: &mut Formatter<'_>) -> Result {
    unsafe { Display::fmt(&*(value as *const T), f) }
}

fn debug_at<T: Debug>(value: *const (), f: &mut Formatter<'_>) -> Result {
    unsafe { Debug::fmt(&*(value as *const T), f) }
}

fn lower_hex_at<T: LowerHex>(value: *const (), f: &mut Formatter<'_>) -> Result {
    unsafe { LowerHex::fmt(&*(value as *const T), f) }
}

fn upper_hex_at<T: UpperHex>(value: *const (), f: &mut Formatter<'_>) -> Result {
    unsafe { UpperHex::fmt(&*(value as *const T), f) }
}

fn octal_at<T: Octal>(value: *const (), f: &mut Formatter<'_>) -> Result {
    unsafe { Octal::fmt(&*(value as *const T), f) }
}

fn binary_at<T: Binary>(value: *const (), f: &mut Formatter<'_>) -> Result {
    unsafe { Binary::fmt(&*(value as *const T), f) }
}

fn lower_exp_at<T: LowerExp>(value: *const (), f: &mut Formatter<'_>) -> Result {
    unsafe { LowerExp::fmt(&*(value as *const T), f) }
}

fn upper_exp_at<T: UpperExp>(value: *const (), f: &mut Formatter<'_>) -> Result {
    unsafe { UpperExp::fmt(&*(value as *const T), f) }
}

fn pointer_at<T: Pointer>(value: *const (), f: &mut Formatter<'_>) -> Result {
    unsafe { Pointer::fmt(&*(value as *const T), f) }
}

/// A count's formatter, never called.
fn no_fmt(_value: *const (), _f: &mut Formatter<'_>) -> Result {
    Ok(())
}

impl<'a> Argument<'a> {
    fn new<T>(x: &'a T, formatter: fn(*const (), &mut Formatter<'_>) -> Result) -> Argument<'a> {
        Argument {
            value: x as *const T as *const (),
            formatter,
            count: 0,
            _lifetime: PhantomData,
        }
    }

    pub fn new_display<T: Display>(x: &'a T) -> Argument<'a> {
        Argument::new(x, display_at::<T>)
    }

    pub fn new_debug<T: Debug>(x: &'a T) -> Argument<'a> {
        Argument::new(x, debug_at::<T>)
    }

    pub fn new_lower_hex<T: LowerHex>(x: &'a T) -> Argument<'a> {
        Argument::new(x, lower_hex_at::<T>)
    }

    pub fn new_upper_hex<T: UpperHex>(x: &'a T) -> Argument<'a> {
        Argument::new(x, upper_hex_at::<T>)
    }

    pub fn new_octal<T: Octal>(x: &'a T) -> Argument<'a> {
        Argument::new(x, octal_at::<T>)
    }

    pub fn new_binary<T: Binary>(x: &'a T) -> Argument<'a> {
        Argument::new(x, binary_at::<T>)
    }

    pub fn new_lower_exp<T: LowerExp>(x: &'a T) -> Argument<'a> {
        Argument::new(x, lower_exp_at::<T>)
    }

    pub fn new_upper_exp<T: UpperExp>(x: &'a T) -> Argument<'a> {
        Argument::new(x, upper_exp_at::<T>)
    }

    pub fn new_pointer<T: Pointer>(x: &'a T) -> Argument<'a> {
        Argument::new(x, pointer_at::<T>)
    }

    /// A width or precision, which rustc keeps in 16 bits.
    #[track_caller]
    pub fn from_usize(x: &usize) -> Argument<'a> {
        if *x > 0xFFFF {
            crate::panicking::panic("Formatting argument out of range");
        }
        Argument {
            value: x as *const usize as *const (),
            formatter: no_fmt,
            count: *x as u16,
            _lifetime: PhantomData,
        }
    }

    pub(super) fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        (self.formatter)(self.value, f)
    }
}

/// A placeholder's options: which argument it shows, its fill,
/// alignment and flags (as `Formatter` keeps them), width and precision.
#[derive(Clone, Copy)]
pub struct Placeholder {
    pub position: usize,
    pub flags: u32,
    pub precision: Count,
    pub width: Count,
}

impl Placeholder {
    pub const fn new(position: usize, flags: u32, precision: Count, width: Count) -> Placeholder {
        Placeholder {
            position,
            flags,
            precision,
            width,
        }
    }
}

/// A width or precision: written in the format string, given by the
/// argument at an index, or none.
#[derive(Clone, Copy)]
pub enum Count {
    Is(u16),
    Param(usize),
    Implied,
}

impl Count {
    pub(super) fn get(&self, args: &[Argument<'_>]) -> Option<u16> {
        match *self {
            Count::Is(n) => Some(n),
            Count::Param(i) => Some(args[i].count),
            Count::Implied => None,
        }
    }
}
