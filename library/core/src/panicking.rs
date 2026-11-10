//! Compiler panic entry points. The runtime owns messages before invoking hooks
//! and either aborting or unwinding. Constant evaluation reports panics directly.

use crate::fmt::{Arguments, Debug, Display};
use crate::option::Option::{self, None, Some};
use crate::panic::Location;

unsafe extern "C" {
    fn abort() -> !;
}

/// The compilation's panic strategy, selected before runtime branches are built.
#[doc(hidden)]
#[rustc_intrinsic]
pub const fn __panic_unwind_enabled() -> bool { false }

/// Calls the callback and returns a caught exception pointer, or null on return.
/// Only the panic runtime may inspect and consume the returned exception.
///
/// # Safety
/// The callback must accept `data` and both must remain valid for the call.
/// A matching runtime must consume a returned exception pointer exactly once.
#[doc(hidden)]
#[rustc_intrinsic]
pub unsafe fn __panic_catch(callback: unsafe extern "C-unwind" fn(*mut u8), data: *mut u8) -> *mut u8 { loop {} }

unsafe extern "C-unwind" {
    fn __flux_panic_str(data: *const u8, len: usize, location: *const Location<'static>) -> !;
    fn __flux_panic_fmt(args: *const Arguments<'_>, location: *const Location<'static>) -> !;
}

/// Prints the default panic message without initiating another panic.
#[doc(hidden)]
pub fn __panic_print(args: Arguments<'_>, location: &Location<'_>) {
    crate::io::print_panic(args, location);
}

/// Panics with `message`.
#[lang = "panic"]
#[track_caller]
#[doc(hidden)]
#[rustc_do_not_const_check]
pub const fn panic(message: &'static str) -> ! {
    // SAFETY: the runtime borrows the static bytes only while owning the payload.
    unsafe { __flux_panic_str(message.as_ptr(), message.len(), Location::caller()) }
}

/// `panic!("{}", x)`.
#[lang = "panic_display"]
#[track_caller]
#[doc(hidden)]
#[rustc_do_not_const_check]
pub const fn panic_display<T: Display>(x: &T) -> ! {
    panic_fmt(format_args!("{}", *x))
}

/// `panic!` with a formatted message, owned by the panic runtime.
#[lang = "panic_fmt"]
#[track_caller]
#[doc(hidden)]
#[rustc_do_not_const_check]
pub const fn panic_fmt(args: Arguments<'_>) -> ! {
    // SAFETY: the runtime formats and owns the payload before unwinding or abort.
    unsafe { __flux_panic_fmt(&args, Location::caller()) }
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

/// Terminates immediately, including when an ownership counter is exhausted.
#[doc(hidden)]
pub fn abort_now() -> ! {
    // SAFETY: C abort has no preconditions and never returns.
    unsafe { abort() }
}
