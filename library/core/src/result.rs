//! Results of operations that may fail: `Result<T, E>` is `Ok(T)` or
//! `Err(E)`.

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
