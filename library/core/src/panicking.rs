//! Panics. flux builds programs with `panic=abort`, so a panic aborts,
//! without its message: flux cannot format one yet. The macros that
//! panic (`panic!`, `assert_eq!`, ...) call these functions.

use crate::fmt::{Arguments, Debug, Display};

unsafe extern "C" {
    fn abort() -> !;
}

/// Aborts the program. Constant evaluation reports it instead, as
/// rustc's "evaluation panicked: " and `message`.
#[lang = "panic"]
#[doc(hidden)]
pub const fn panic<M>(message: M) -> ! {
    unsafe { abort() }
}

/// `panic!("{}", x)`.
#[doc(hidden)]
pub const fn panic_display<T: Display>(x: &T) -> ! {
    panic("explicit panic")
}

/// `panic!` with a formatted message.
#[doc(hidden)]
pub const fn panic_fmt(args: Arguments<'_>) -> ! {
    panic("explicit panic")
}

/// Which assertion failed, for `assert_failed`.
#[doc(hidden)]
pub enum AssertKind {
    Eq,
    Ne,
    Match,
}

/// A failed `assert_eq!` or `assert_ne!`, whose operands must implement
/// `Debug`.
#[doc(hidden)]
pub fn assert_failed<T: Debug, U: Debug>(
    kind: AssertKind,
    left: &T,
    right: &U,
    args: Option<Arguments<'_>>,
) -> ! {
    panic("assertion failed")
}
