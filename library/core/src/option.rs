//! Optional values: `Option<T>` is `Some(T)` or `None`.

use crate::ops::FnOnce;
use crate::panicking::panic;

/// A value that may be absent. `?` returns `None` from a function
/// that returns an `Option` if it is `None`.
#[lang = "Option"]
pub enum Option<T> {
    None,
    Some(T),
}

use Option::{None, Some};

impl<T> Option<T> {
    /// Whether it is `Some`.
    pub const fn is_some(&self) -> bool {
        match self {
            Some(_) => true,
            None => false,
        }
    }

    /// Whether it is `None`.
    pub const fn is_none(&self) -> bool {
        match self {
            Some(_) => false,
            None => true,
        }
    }

    /// The value; panics on `None`.
    #[rustc_allow_const_fn_unstable(const_precise_live_drops)]
    pub const fn unwrap(self) -> T {
        match self {
            Some(value) => value,
            None => panic(c"called `Option::unwrap()` on a `None` value"),
        }
    }

    /// The value, or `default` on `None`.
    #[rustc_const_unstable]
    pub fn unwrap_or(self, default: T) -> T {
        match self {
            Some(value) => value,
            None => default,
        }
    }

    /// `Ok` with the value, or `Err(err)` on `None`.
    #[rustc_const_unstable]
    pub fn ok_or<E>(self, err: E) -> crate::result::Result<T, E> {
        match self {
            Some(value) => crate::result::Result::Ok(value),
            None => crate::result::Result::Err(err),
        }
    }

    /// `optb` if both are `Some`, else `None`.
    #[rustc_const_unstable]
    pub fn and<U>(self, optb: Option<U>) -> Option<U> {
        match self {
            Some(_) => optb,
            None => None,
        }
    }

    /// Itself if it is `Some`, else `optb`.
    #[rustc_const_unstable]
    pub fn or(self, optb: Option<T>) -> Option<T> {
        match self {
            Some(value) => Some(value),
            None => optb,
        }
    }

    /// `Some` if exactly one of the two is.
    #[rustc_const_unstable]
    pub fn xor(self, optb: Option<T>) -> Option<T> {
        match self {
            Some(a) => match optb {
                Some(_) => None,
                None => Some(a),
            },
            None => optb,
        }
    }

    /// Whether it is `Some` and `f` is `true` for the value.
    pub fn is_some_and<F: FnOnce(T) -> bool>(self, f: F) -> bool {
        match self {
            Some(value) => f(value),
            None => false,
        }
    }

    /// Whether it is `None`, or `f` is `true` for the value.
    pub fn is_none_or<F: FnOnce(T) -> bool>(self, f: F) -> bool {
        match self {
            Some(value) => f(value),
            None => true,
        }
    }

    /// `Some(f(value))`, or `None`.
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Option<U> {
        match self {
            Some(value) => Some(f(value)),
            None => None,
        }
    }

    /// Calls `f` with a reference to the value, if any, and gives itself.
    pub fn inspect<F: FnOnce(&T)>(self, f: F) -> Self {
        if let Some(value) = &self {
            f(value);
        }
        self
    }

    /// `f(value)`, or `default` on `None`.
    pub fn map_or<U, F: FnOnce(T) -> U>(self, default: U, f: F) -> U {
        match self {
            Some(value) => f(value),
            None => default,
        }
    }

    /// `f(value)`, or `default()` on `None`.
    pub fn map_or_else<U, D: FnOnce() -> U, F: FnOnce(T) -> U>(self, default: D, f: F) -> U {
        match self {
            Some(value) => f(value),
            None => default(),
        }
    }

    /// `Ok` with the value, or `Err(err())` on `None`.
    pub fn ok_or_else<E, F: FnOnce() -> E>(self, err: F) -> crate::result::Result<T, E> {
        match self {
            Some(value) => crate::result::Result::Ok(value),
            None => crate::result::Result::Err(err()),
        }
    }

    /// `f(value)`, or `None`.
    pub fn and_then<U, F: FnOnce(T) -> Option<U>>(self, f: F) -> Option<U> {
        match self {
            Some(value) => f(value),
            None => None,
        }
    }

    /// Itself if it is `Some` and `predicate` is `true` for the value,
    /// else `None`.
    pub fn filter<P: FnOnce(&T) -> bool>(self, predicate: P) -> Self {
        if let Some(value) = self {
            if predicate(&value) {
                return Some(value);
            }
        }
        None
    }

    /// Itself if it is `Some`, else `f()`.
    pub fn or_else<F: FnOnce() -> Option<T>>(self, f: F) -> Option<T> {
        match self {
            Some(value) => Some(value),
            None => f(),
        }
    }

    /// The value, or `f()` on `None`.
    pub fn unwrap_or_else<F: FnOnce() -> T>(self, f: F) -> T {
        match self {
            Some(value) => value,
            None => f(),
        }
    }

    /// `Some((a, b))` if both are `Some`.
    pub fn zip<U>(self, other: Option<U>) -> Option<(T, U)> {
        match self {
            Some(a) => match other {
                Some(b) => Some((a, b)),
                None => None,
            },
            None => None,
        }
    }
}

impl<T: crate::clone::Clone> crate::clone::Clone for Option<T> {
    fn clone(&self) -> Self {
        match self {
            Some(value) => Some(value.clone()),
            None => None,
        }
    }
}

impl<T: crate::marker::Copy> crate::marker::Copy for Option<T> {}

impl<T: crate::cmp::PartialEq> crate::cmp::PartialEq for Option<T> {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Some(a) => match other {
                Some(b) => *a == *b,
                None => false,
            },
            None => match other {
                Some(_) => false,
                None => true,
            },
        }
    }
}

impl<T: crate::cmp::Eq> crate::cmp::Eq for Option<T> {}

// `None` is less than any `Some`, as rustc's derived impls order them.
impl<T: crate::cmp::PartialOrd> crate::cmp::PartialOrd for Option<T> {
    fn partial_cmp(&self, other: &Self) -> Option<crate::cmp::Ordering> {
        match self {
            Some(a) => match other {
                Some(b) => crate::cmp::PartialOrd::partial_cmp(a, b),
                None => Some(crate::cmp::Ordering::Greater),
            },
            None => match other {
                Some(_) => Some(crate::cmp::Ordering::Less),
                None => Some(crate::cmp::Ordering::Equal),
            },
        }
    }
}

impl<T: crate::cmp::Ord> crate::cmp::Ord for Option<T> {
    fn cmp(&self, other: &Self) -> crate::cmp::Ordering {
        match self {
            Some(a) => match other {
                Some(b) => crate::cmp::Ord::cmp(a, b),
                None => crate::cmp::Ordering::Greater,
            },
            None => match other {
                Some(_) => crate::cmp::Ordering::Less,
                None => crate::cmp::Ordering::Equal,
            },
        }
    }
}
