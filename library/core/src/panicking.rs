//! Panics. flux builds programs with `panic=abort`, so a panic prints
//! its message and where it happened to stderr, as std's default hook
//! does, and aborts. The macros that panic (`panic!`, `assert_eq!`,
//! ...) call these functions, and so do the compiler's checks of
//! indices and divisors. Constant evaluation reports a panic with a
//! message it knows instead, as rustc's "evaluation panicked: ".

use crate::fmt::{Arguments, Debug, Display};
use crate::option::Option::{self, None, Some};
use crate::panic::Location;

unsafe extern "C" {
    fn abort() -> !;
}

/// Panics with `message`.
#[lang = "panic"]
#[track_caller]
#[doc(hidden)]
#[rustc_do_not_const_check]
pub const fn panic(message: &'static str) -> ! {
    panic_fmt(Arguments::new_const(&[message]))
}

/// `panic!("{}", x)`.
#[lang = "panic_display"]
#[track_caller]
#[doc(hidden)]
#[rustc_do_not_const_check]
pub const fn panic_display<T: Display>(x: &T) -> ! {
    panic_fmt(format_args!("{}", *x))
}

/// `panic!` with a formatted message: prints it and aborts.
#[lang = "panic_fmt"]
#[track_caller]
#[doc(hidden)]
#[rustc_do_not_const_check]
pub const fn panic_fmt(args: Arguments<'_>) -> ! {
    crate::io::print_panic(args, Location::caller());
    unsafe { abort() }
}

/// `Option::expect` and `Result::expect` with message `msg`.
#[track_caller]
#[doc(hidden)]
pub const fn panic_str(msg: &str) -> ! {
    panic_display(&msg)
}

/// A failed bounds check: `a[i]` with `i` past the end.
#[lang = "panic_bounds_check"]
#[track_caller]
#[doc(hidden)]
pub fn panic_bounds_check(index: usize, len: usize) -> ! {
    panic_fmt(format_args!(
        "index out of bounds: the len is {} but the index is {}",
        len, index
    ))
}

/// The compiler's other checks, each a function as rustc's are.
macro_rules! panic_const {
    ($($name:ident $lang:literal = $message:literal,)*) => {
        $(
            #[lang = $lang]
            #[track_caller]
            #[doc(hidden)]
            pub fn $name() -> ! {
                panic_fmt(Arguments::new_const(&[$message]))
            }
        )*
    };
}

panic_const! {
    panic_const_add_overflow "panic_const_add_overflow" = "attempt to add with overflow",
    panic_const_sub_overflow "panic_const_sub_overflow" = "attempt to subtract with overflow",
    panic_const_mul_overflow "panic_const_mul_overflow" = "attempt to multiply with overflow",
    panic_const_div_overflow "panic_const_div_overflow" = "attempt to divide with overflow",
    panic_const_rem_overflow "panic_const_rem_overflow" = "attempt to calculate the remainder with overflow",
    panic_const_neg_overflow "panic_const_neg_overflow" = "attempt to negate with overflow",
    panic_const_shr_overflow "panic_const_shr_overflow" = "attempt to shift right with overflow",
    panic_const_shl_overflow "panic_const_shl_overflow" = "attempt to shift left with overflow",
    panic_const_div_by_zero "panic_const_div_by_zero" = "attempt to divide by zero",
    panic_const_rem_by_zero "panic_const_rem_by_zero" = "attempt to calculate the remainder with a divisor of zero",
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
#[track_caller]
#[doc(hidden)]
pub fn assert_failed<T: Debug, U: Debug>(
    kind: AssertKind,
    left: &T,
    right: &U,
    args: Option<Arguments<'_>>,
) -> ! {
    let op = match kind {
        AssertKind::Eq => "==",
        AssertKind::Ne => "!=",
        AssertKind::Match => "matches",
    };
    match args {
        Some(args) => panic_fmt(format_args!(
            "assertion `left {op} right` failed: {args}\n  left: {left:?}\n right: {right:?}"
        )),
        None => panic_fmt(format_args!(
            "assertion `left {op} right` failed\n  left: {left:?}\n right: {right:?}"
        )),
    }
}

/// `Result::unwrap` and friends' message, with the value they did not
/// expect.
#[track_caller]
#[doc(hidden)]
pub fn unwrap_failed<E: Debug>(msg: &str, error: &E) -> ! {
    panic_fmt(format_args!("{msg}: {error:?}"))
}
