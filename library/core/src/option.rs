//! Optional values: `Option<T>` is `Some(T)` or `None`.

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
