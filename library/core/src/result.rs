//! Results of operations that may fail: `Result<T, E>` is `Ok(T)` or
//! `Err(E)`.

use crate::ops::FnOnce;
use crate::option::Option::{self, None, Some};

/// A value, or an error. `?` returns the error from a function that
/// returns a `Result` if it is `Err`, converted with `From`.
#[lang = "Result"]
pub enum Result<T, E> {
    Ok(T),
    Err(E),
}

use Result::{Err, Ok};

impl<T, E> Result<T, E> {
    /// Whether it is `Ok`.
    pub const fn is_ok(&self) -> bool {
        match self {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    /// Whether it is `Err`.
    pub const fn is_err(&self) -> bool {
        match self {
            Ok(_) => false,
            Err(_) => true,
        }
    }

    /// The value, if `Ok`.
    #[rustc_const_unstable]
    pub fn ok(self) -> Option<T> {
        match self {
            Ok(value) => Some(value),
            Err(_) => None,
        }
    }

    /// The error, if `Err`.
    #[rustc_const_unstable]
    pub fn err(self) -> Option<E> {
        match self {
            Ok(_) => None,
            Err(err) => Some(err),
        }
    }

    /// The value, or `default` on `Err`.
    #[rustc_const_unstable]
    pub fn unwrap_or(self, default: T) -> T {
        match self {
            Ok(value) => value,
            Err(_) => default,
        }
    }

    /// `res` if it is `Ok`, else its error.
    #[rustc_const_unstable]
    pub fn and<U>(self, res: Result<U, E>) -> Result<U, E> {
        match self {
            Ok(_) => res,
            Err(err) => Err(err),
        }
    }

    /// Itself if it is `Ok`, else `res`.
    #[rustc_const_unstable]
    pub fn or<F>(self, res: Result<T, F>) -> Result<T, F> {
        match self {
            Ok(value) => Ok(value),
            Err(_) => res,
        }
    }

    /// Whether it is `Ok` and `f` is `true` for the value.
    pub fn is_ok_and<F: FnOnce(T) -> bool>(self, f: F) -> bool {
        match self {
            Ok(value) => f(value),
            Err(_) => false,
        }
    }

    /// Whether it is `Err` and `f` is `true` for the error.
    pub fn is_err_and<F: FnOnce(E) -> bool>(self, f: F) -> bool {
        match self {
            Ok(_) => false,
            Err(err) => f(err),
        }
    }

    /// `Ok(op(value))`, or the error.
    pub fn map<U, F: FnOnce(T) -> U>(self, op: F) -> Result<U, E> {
        match self {
            Ok(value) => Ok(op(value)),
            Err(err) => Err(err),
        }
    }

    /// `f(value)`, or `default` on an error.
    pub fn map_or<U, F: FnOnce(T) -> U>(self, default: U, f: F) -> U {
        match self {
            Ok(value) => f(value),
            Err(_) => default,
        }
    }

    /// `f(value)`, or `default(err)` on an error.
    pub fn map_or_else<U, D: FnOnce(E) -> U, F: FnOnce(T) -> U>(self, default: D, f: F) -> U {
        match self {
            Ok(value) => f(value),
            Err(err) => default(err),
        }
    }

    /// The value, or `Err(op(err))`.
    pub fn map_err<F, O: FnOnce(E) -> F>(self, op: O) -> Result<T, F> {
        match self {
            Ok(value) => Ok(value),
            Err(err) => Err(op(err)),
        }
    }

    /// `op(value)`, or the error.
    pub fn and_then<U, F: FnOnce(T) -> Result<U, E>>(self, op: F) -> Result<U, E> {
        match self {
            Ok(value) => op(value),
            Err(err) => Err(err),
        }
    }

    /// Itself if it is `Ok`, else `op(err)`.
    pub fn or_else<F, O: FnOnce(E) -> Result<T, F>>(self, op: O) -> Result<T, F> {
        match self {
            Ok(value) => Ok(value),
            Err(err) => op(err),
        }
    }

    /// The value, or `op(err)` on an error.
    pub fn unwrap_or_else<F: FnOnce(E) -> T>(self, op: F) -> T {
        match self {
            Ok(value) => value,
            Err(err) => op(err),
        }
    }
}

impl<T: crate::clone::Clone, E: crate::clone::Clone> crate::clone::Clone for Result<T, E> {
    fn clone(&self) -> Self {
        match self {
            Ok(value) => Ok(value.clone()),
            Err(err) => Err(err.clone()),
        }
    }
}

impl<T: crate::marker::Copy, E: crate::marker::Copy> crate::marker::Copy for Result<T, E> {}

impl<T: crate::cmp::PartialEq, E: crate::cmp::PartialEq> crate::cmp::PartialEq for Result<T, E> {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Ok(a) => match other {
                Ok(b) => *a == *b,
                Err(_) => false,
            },
            Err(a) => match other {
                Ok(_) => false,
                Err(b) => *a == *b,
            },
        }
    }
}

impl<T: crate::cmp::Eq, E: crate::cmp::Eq> crate::cmp::Eq for Result<T, E> {}

// `Ok` is less than any `Err`, as rustc's derived impls order them.
impl<T: crate::cmp::PartialOrd, E: crate::cmp::PartialOrd> crate::cmp::PartialOrd for Result<T, E> {
    fn partial_cmp(&self, other: &Self) -> Option<crate::cmp::Ordering> {
        match self {
            Ok(a) => match other {
                Ok(b) => crate::cmp::PartialOrd::partial_cmp(a, b),
                Err(_) => Some(crate::cmp::Ordering::Less),
            },
            Err(a) => match other {
                Ok(_) => Some(crate::cmp::Ordering::Greater),
                Err(b) => crate::cmp::PartialOrd::partial_cmp(a, b),
            },
        }
    }
}

impl<T: crate::cmp::Ord, E: crate::cmp::Ord> crate::cmp::Ord for Result<T, E> {
    fn cmp(&self, other: &Self) -> crate::cmp::Ordering {
        match self {
            Ok(a) => match other {
                Ok(b) => crate::cmp::Ord::cmp(a, b),
                Err(_) => crate::cmp::Ordering::Less,
            },
            Err(a) => match other {
                Ok(_) => crate::cmp::Ordering::Greater,
                Err(b) => crate::cmp::Ord::cmp(a, b),
            },
        }
    }
}
