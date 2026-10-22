//! Macros, as rustc's `core` defines them: those the compiler
//! implements (`#[rustc_builtin_macro]`), whose rules here are never
//! used, and `macro_rules!` macros built on them.

#[macro_export]
#[rustc_builtin_macro]
macro_rules! assert {
    ($cond:expr $(,)?) => {{ /* compiler built-in */ }};
    ($cond:expr, $($arg:tt)+) => {{ /* compiler built-in */ }};
}

/// Panics, which aborts: rustc's `panic_2021!`.
#[macro_export]
#[rustc_builtin_macro]
macro_rules! panic {
    ($($arg:tt)*) => {
        /* compiler built-in */
    };
}

/// Panics with "internal error: entered unreachable code": rustc's
/// `unreachable_2021!`.
#[macro_export]
#[rustc_builtin_macro]
macro_rules! unreachable {
    ($($arg:tt)*) => {
        /* compiler built-in */
    };
}

#[macro_export]
#[rustc_builtin_macro]
macro_rules! format_args {
    ($fmt:expr) => {{ /* compiler built-in */ }};
    ($fmt:expr, $($args:tt)*) => {{ /* compiler built-in */ }};
}

#[macro_export]
#[rustc_builtin_macro]
macro_rules! const_format_args {
    ($fmt:expr) => {{ /* compiler built-in */ }};
    ($fmt:expr, $($args:tt)*) => {{ /* compiler built-in */ }};
}

#[macro_export]
#[rustc_builtin_macro]
macro_rules! format_args_nl {
    ($fmt:expr) => {{ /* compiler built-in */ }};
    ($fmt:expr, $($args:tt)*) => {{ /* compiler built-in */ }};
}

#[macro_export]
#[rustc_builtin_macro]
macro_rules! concat {
    ($($e:expr),* $(,)?) => {{ /* compiler built-in */ }};
}

#[macro_export]
#[rustc_builtin_macro]
macro_rules! stringify {
    ($($t:tt)*) => {
        /* compiler built-in */
    };
}

#[macro_export]
#[rustc_builtin_macro]
macro_rules! line {
    () => {
        /* compiler built-in */
    };
}

#[macro_export]
#[rustc_builtin_macro]
macro_rules! column {
    () => {
        /* compiler built-in */
    };
}

#[macro_export]
#[rustc_builtin_macro]
macro_rules! file {
    () => {
        /* compiler built-in */
    };
}

#[macro_export]
#[rustc_builtin_macro]
macro_rules! module_path {
    () => {
        /* compiler built-in */
    };
}

#[macro_export]
#[rustc_builtin_macro]
macro_rules! cfg {
    ($($cfg:tt)*) => {
        /* compiler built-in */
    };
}

#[macro_export]
#[rustc_builtin_macro]
macro_rules! compile_error {
    ($msg:expr $(,)?) => {{ /* compiler built-in */ }};
}

#[macro_export]
#[rustc_builtin_macro]
macro_rules! env {
    ($($t:tt)*) => {{ /* compiler built-in */ }};
}

#[macro_export]
#[rustc_builtin_macro]
macro_rules! option_env {
    ($($t:tt)*) => {{ /* compiler built-in */ }};
}

#[macro_export]
#[rustc_builtin_macro]
macro_rules! include {
    ($($t:tt)*) => {{ /* compiler built-in */ }};
}

#[macro_export]
#[rustc_builtin_macro]
macro_rules! include_str {
    ($($t:tt)*) => {{ /* compiler built-in */ }};
}

#[macro_export]
#[rustc_builtin_macro]
macro_rules! include_bytes {
    ($($t:tt)*) => {{ /* compiler built-in */ }};
}

#[macro_export]
macro_rules! matches {
    ($expression:expr, $pattern:pat $(if $guard:expr)? $(,)?) => {
        #[allow(non_exhaustive_omitted_patterns)]
        match $expression {
            $pattern $(if $guard)? => true,
            _ => false
        }
    };
}

#[macro_export]
macro_rules! assert_eq {
    ($left:expr, $right:expr $(,)?) => {{
        match (&$left, &$right) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = $crate::panicking::AssertKind::Eq;
                    $crate::panicking::assert_failed(kind, &*left_val, &*right_val, $crate::option::Option::None);
                }
            }
        }
    }};
    ($left:expr, $right:expr, $($arg:tt)+) => {{
        match (&$left, &$right) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = $crate::panicking::AssertKind::Eq;
                    $crate::panicking::assert_failed(kind, &*left_val, &*right_val, $crate::option::Option::Some($crate::format_args!($($arg)+)));
                }
            }
        }
    }};
}

#[macro_export]
macro_rules! assert_ne {
    ($left:expr, $right:expr $(,)?) => {{
        match (&$left, &$right) {
            (left_val, right_val) => {
                if *left_val == *right_val {
                    let kind = $crate::panicking::AssertKind::Ne;
                    $crate::panicking::assert_failed(kind, &*left_val, &*right_val, $crate::option::Option::None);
                }
            }
        }
    }};
    ($left:expr, $right:expr, $($arg:tt)+) => {{
        match (&($left), &($right)) {
            (left_val, right_val) => {
                if *left_val == *right_val {
                    let kind = $crate::panicking::AssertKind::Ne;
                    $crate::panicking::assert_failed(kind, &*left_val, &*right_val, $crate::option::Option::Some($crate::format_args!($($arg)+)));
                }
            }
        }
    }};
}

/// `assert!` in debug builds: flux's are like rustc's without `-O`, so
/// always.
#[macro_export]
macro_rules! debug_assert {
    ($($arg:tt)*) => {
        if $crate::cfg!(debug_assertions) {
            $crate::assert!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! debug_assert_eq {
    ($($arg:tt)*) => {
        if $crate::cfg!(debug_assertions) {
            $crate::assert_eq!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! debug_assert_ne {
    ($($arg:tt)*) => {
        if $crate::cfg!(debug_assertions) {
            $crate::assert_ne!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! unimplemented {
    () => {
        $crate::panicking::panic("not implemented")
    };
    ($($arg:tt)+) => {
        $crate::panic!("not implemented: {}", $crate::format_args!($($arg)+))
    };
}

#[macro_export]
macro_rules! todo {
    () => {
        $crate::panicking::panic("not yet implemented")
    };
    ($($arg:tt)+) => {
        $crate::panic!("not yet implemented: {}", $crate::format_args!($($arg)+))
    };
}

/// Writes formatted text to a `fmt::Write`, or anything with a
/// `write_fmt` method.
#[macro_export]
macro_rules! write {
    ($dst:expr, $($arg:tt)*) => {
        $dst.write_fmt($crate::format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! writeln {
    ($dst:expr $(,)?) => {
        $crate::write!($dst, "\n")
    };
    ($dst:expr, $($arg:tt)*) => {
        $dst.write_fmt($crate::format_args_nl!($($arg)*))
    };
}

// `std`'s printing macros, which rustc's programs get from `std`'s
// prelude: every program sees them by name, and as `std::println!`,
// but not as `core::println!` (see `flux_hir::macros`), so they do not
// name each other through `$crate`.

/// Prints to stdout.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        $crate::io::_print($crate::format_args!($($arg)*));
    }};
}

/// Prints to stdout, with a newline.
#[macro_export]
macro_rules! println {
    () => {{
        $crate::io::_print($crate::format_args!("\n"));
    }};
    ($($arg:tt)*) => {{
        $crate::io::_print($crate::format_args_nl!($($arg)*));
    }};
}

/// Prints to stderr.
#[macro_export]
macro_rules! eprint {
    ($($arg:tt)*) => {{
        $crate::io::_eprint($crate::format_args!($($arg)*));
    }};
}

/// Prints to stderr, with a newline.
#[macro_export]
macro_rules! eprintln {
    () => {{
        $crate::io::_eprint($crate::format_args!("\n"));
    }};
    ($($arg:tt)*) => {{
        $crate::io::_eprint($crate::format_args_nl!($($arg)*));
    }};
}

/// Prints each expression and its value (`{:#?}`) to stderr, with
/// where it is, and gives the value (a tuple of them for several).
#[macro_export]
macro_rules! dbg {
    () => {
        $crate::io::_eprint($crate::format_args_nl!(
            "[{}:{}:{}]", $crate::file!(), $crate::line!(), $crate::column!()
        ))
    };
    ($val:expr $(,)?) => {
        match $val {
            tmp => {
                $crate::io::_eprint($crate::format_args_nl!(
                    "[{}:{}:{}] {} = {:#?}",
                    $crate::file!(), $crate::line!(), $crate::column!(), $crate::stringify!($val), &tmp
                ));
                tmp
            }
        }
    };
    ($($val:expr),+ $(,)?) => {
        ($(
            match $val {
                tmp => {
                    $crate::io::_eprint($crate::format_args_nl!(
                        "[{}:{}:{}] {} = {:#?}",
                        $crate::file!(), $crate::line!(), $crate::column!(), $crate::stringify!($val), &tmp
                    ));
                    tmp
                }
            }
        ),+,)
    };
}
