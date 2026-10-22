//! Optional values: `Option<T>` is `Some(T)` or `None`.

use crate::ops::FnOnce;
use crate::panicking::panic;

/// A value that may be absent. `?` returns `None` from a function
/// that returns an `Option` if it is `None`.
#[lang = "Option"]
#[derive(Debug, Hash)]
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
    #[track_caller]
    pub const fn unwrap(self) -> T {
        match self {
            Some(value) => value,
            None => panic("called `Option::unwrap()` on a `None` value"),
        }
    }

    /// The value; panics with `msg` on `None`.
    #[rustc_allow_const_fn_unstable(const_precise_live_drops)]
    #[track_caller]
    pub const fn expect(self, msg: &str) -> T {
        match self {
            Some(value) => value,
            None => crate::panicking::panic_str(msg),
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

    /// The value, or `T`'s default on `None`.
    #[rustc_const_unstable]
    pub fn unwrap_or_default(self) -> T
    where
        T: crate::default::Default,
    {
        match self {
            Some(value) => value,
            None => T::default(),
        }
    }

    /// The value, after putting `T`'s default in if it is `None`.
    #[rustc_const_unstable]
    pub fn get_or_insert_default(&mut self) -> &mut T
    where
        T: crate::default::Default,
    {
        self.get_or_insert_with(T::default)
    }

    /// A reference to the value, if any.
    pub const fn as_ref(&self) -> Option<&T> {
        match self {
            Some(value) => Some(value),
            None => None,
        }
    }

    /// A mutable reference to the value, if any.
    pub const fn as_mut(&mut self) -> Option<&mut T> {
        match self {
            Some(value) => Some(value),
            None => None,
        }
    }

    /// Takes the value out, leaving `None`. (`const` in rustc; flux's
    /// constant evaluation cannot write through `&mut` yet.)
    pub fn take(&mut self) -> Option<T> {
        crate::mem::replace(self, None)
    }

    /// Takes the value out if `predicate` is `true` for it, leaving `None`.
    pub fn take_if<P: FnOnce(&mut T) -> bool>(&mut self, predicate: P) -> Option<T> {
        let taken = match self {
            Some(value) => predicate(value),
            None => false,
        };
        if taken { self.take() } else { None }
    }

    /// Puts `value` in, giving back what was there. (`const` in rustc.)
    pub fn replace(&mut self, value: T) -> Option<T> {
        crate::mem::replace(self, Some(value))
    }

    /// Puts `value` in, and gives a reference to it.
    #[rustc_const_unstable]
    #[track_caller]
    pub fn insert(&mut self, value: T) -> &mut T {
        *self = Some(value);
        match self {
            Some(value) => value,
            None => crate::panicking::panic("internal error: entered unreachable code"),
        }
    }

    /// The value, after putting `value` in if it is `None`.
    #[rustc_const_unstable]
    #[track_caller]
    pub fn get_or_insert(&mut self, value: T) -> &mut T {
        if let None = self {
            *self = Some(value);
        }
        match self {
            Some(value) => value,
            None => crate::panicking::panic("internal error: entered unreachable code"),
        }
    }

    /// The value, after putting `f()` in if it is `None`.
    #[track_caller]
    pub fn get_or_insert_with<F: FnOnce() -> T>(&mut self, f: F) -> &mut T {
        if let None = self {
            *self = Some(f());
        }
        match self {
            Some(value) => value,
            None => crate::panicking::panic("internal error: entered unreachable code"),
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

impl<T> Option<&T> {
    /// `Some` with a copy of the value it refers to, or `None`.
    pub const fn copied(self) -> Option<T>
    where
        T: crate::marker::Copy,
    {
        match self {
            Some(&value) => Some(value),
            None => None,
        }
    }

    /// `Some` with a clone of the value it refers to, or `None`.
    pub fn cloned(self) -> Option<T>
    where
        T: crate::clone::Clone,
    {
        match self {
            Some(value) => Some(value.clone()),
            None => None,
        }
    }
}

impl<T> Option<&mut T> {
    /// `Some` with a copy of the value it refers to, or `None`.
    pub const fn copied(self) -> Option<T>
    where
        T: crate::marker::Copy,
    {
        match self {
            Some(&mut value) => Some(value),
            None => None,
        }
    }

    /// `Some` with a clone of the value it refers to, or `None`.
    pub fn cloned(self) -> Option<T>
    where
        T: crate::clone::Clone,
    {
        match self {
            Some(value) => Some(value.clone()),
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

impl<T> Option<T> {
    /// An iterator over a reference to the value, if any.
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            inner: self.as_ref(),
        }
    }
}

/// An `Option` is an iterator of its value, if any.
impl<T> crate::iter::IntoIterator for Option<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> IntoIter<T> {
        IntoIter { inner: self }
    }
}

impl<'a, T> crate::iter::IntoIterator for &'a Option<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}

/// [`Option::into_iter`]'s.
pub struct IntoIter<T> {
    inner: Option<T>,
}

impl<T: crate::clone::Clone> crate::clone::Clone for IntoIter<T> {
    fn clone(&self) -> Self {
        IntoIter {
            inner: self.inner.clone(),
        }
    }
}

impl<T> crate::iter::Iterator for IntoIter<T> {
    type Item = T;
    const __TRUSTED_LEN: bool = true;

    fn next(&mut self) -> Option<T> {
        self.inner.take()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = if self.inner.is_some() { 1 } else { 0 };
        (n, Some(n))
    }
}

impl<T> crate::iter::DoubleEndedIterator for IntoIter<T> {
    fn next_back(&mut self) -> Option<T> {
        self.inner.take()
    }
}

impl<T> crate::iter::ExactSizeIterator for IntoIter<T> {}

/// [`Option::iter`]'s.
pub struct Iter<'a, T> {
    inner: Option<&'a T>,
}

impl<'a, T> crate::clone::Clone for Iter<'a, T> {
    fn clone(&self) -> Self {
        Iter { inner: self.inner }
    }
}

impl<'a, T> crate::iter::Iterator for Iter<'a, T> {
    type Item = &'a T;
    const __TRUSTED_LEN: bool = true;

    fn next(&mut self) -> Option<&'a T> {
        self.inner.take()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = if self.inner.is_some() { 1 } else { 0 };
        (n, Some(n))
    }
}

impl<'a, T> crate::iter::DoubleEndedIterator for Iter<'a, T> {
    fn next_back(&mut self) -> Option<&'a T> {
        self.inner.take()
    }
}

impl<'a, T> crate::iter::ExactSizeIterator for Iter<'a, T> {}

impl<T> Option<Option<T>> {
    /// The inner option: `None` if either is.
    pub fn flatten(self) -> Option<T> {
        match self {
            Some(inner) => inner,
            None => None,
        }
    }
}
