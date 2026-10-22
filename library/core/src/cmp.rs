//! Comparison: `a == b` and `a != b` on types other than integers and
//! `bool` are `PartialEq::eq(&a, &b)` and `PartialEq::ne(&a, &b)`, and
//! `a < b` is `PartialOrd::lt(&a, &b)` (`le`, `gt` and `ge` for `<=`, `>`
//! and `>=`).

use crate::marker::Sized;
use crate::option::Option::{self, None, Some};
use crate::panicking::panic;
use Ordering::{Equal, Greater, Less};

/// The `==` and `!=` operators.
#[lang = "eq"]
#[diagnostic::on_unimplemented(message = "can't compare `{Self}` with `{Rhs}`")]
pub trait PartialEq<Rhs = Self> {
    fn eq(&self, other: &Rhs) -> bool;
    fn ne(&self, other: &Rhs) -> bool {
        !self.eq(other)
    }
}

/// Equality that is reflexive: `a == a` for every `a`.
#[lang = "total_eq"]
pub trait Eq: PartialEq {
    /// What `#[derive(Eq)]` defines, to check that each field is `Eq`.
    #[doc(hidden)]
    fn assert_receiver_is_total_eq(&self) {}
}

impl PartialEq for i8 { fn eq(&self, other: &i8) -> bool { *self == *other } fn ne(&self, other: &i8) -> bool { *self != *other } }
impl Eq for i8 {}
impl PartialEq for i16 { fn eq(&self, other: &i16) -> bool { *self == *other } fn ne(&self, other: &i16) -> bool { *self != *other } }
impl Eq for i16 {}
impl PartialEq for i32 { fn eq(&self, other: &i32) -> bool { *self == *other } fn ne(&self, other: &i32) -> bool { *self != *other } }
impl Eq for i32 {}
impl PartialEq for i64 { fn eq(&self, other: &i64) -> bool { *self == *other } fn ne(&self, other: &i64) -> bool { *self != *other } }
impl Eq for i64 {}
impl PartialEq for i128 { fn eq(&self, other: &i128) -> bool { *self == *other } fn ne(&self, other: &i128) -> bool { *self != *other } }
impl Eq for i128 {}
impl PartialEq for isize { fn eq(&self, other: &isize) -> bool { *self == *other } fn ne(&self, other: &isize) -> bool { *self != *other } }
impl Eq for isize {}
impl PartialEq for u8 { fn eq(&self, other: &u8) -> bool { *self == *other } fn ne(&self, other: &u8) -> bool { *self != *other } }
impl Eq for u8 {}
impl PartialEq for u16 { fn eq(&self, other: &u16) -> bool { *self == *other } fn ne(&self, other: &u16) -> bool { *self != *other } }
impl Eq for u16 {}
impl PartialEq for u32 { fn eq(&self, other: &u32) -> bool { *self == *other } fn ne(&self, other: &u32) -> bool { *self != *other } }
impl Eq for u32 {}
impl PartialEq for u64 { fn eq(&self, other: &u64) -> bool { *self == *other } fn ne(&self, other: &u64) -> bool { *self != *other } }
impl Eq for u64 {}
impl PartialEq for u128 { fn eq(&self, other: &u128) -> bool { *self == *other } fn ne(&self, other: &u128) -> bool { *self != *other } }
impl Eq for u128 {}
impl PartialEq for usize { fn eq(&self, other: &usize) -> bool { *self == *other } fn ne(&self, other: &usize) -> bool { *self != *other } }
impl Eq for usize {}
impl PartialEq for bool { fn eq(&self, other: &bool) -> bool { *self == *other } fn ne(&self, other: &bool) -> bool { *self != *other } }
impl Eq for bool {}
impl PartialEq for char { fn eq(&self, other: &char) -> bool { *self == *other } fn ne(&self, other: &char) -> bool { *self != *other } }
impl Eq for char {}
impl PartialEq for () { fn eq(&self, _other: &()) -> bool { true } }
impl Eq for () {}

// References compare what they point to, as in rustc: `&a == &b` is
// `a == b`, whichever of them are mutable.
impl<A: PartialEq<B>, B> PartialEq<&B> for &A {
    fn eq(&self, other: &&B) -> bool {
        PartialEq::eq(&**self, &**other)
    }
    fn ne(&self, other: &&B) -> bool {
        PartialEq::ne(&**self, &**other)
    }
}

impl<A: PartialEq<B>, B> PartialEq<&mut B> for &mut A {
    fn eq(&self, other: &&mut B) -> bool {
        PartialEq::eq(&**self, &**other)
    }
    fn ne(&self, other: &&mut B) -> bool {
        PartialEq::ne(&**self, &**other)
    }
}

impl<A: PartialEq<B>, B> PartialEq<&mut B> for &A {
    fn eq(&self, other: &&mut B) -> bool {
        PartialEq::eq(&**self, &**other)
    }
    fn ne(&self, other: &&mut B) -> bool {
        PartialEq::ne(&**self, &**other)
    }
}

impl<A: PartialEq<B>, B> PartialEq<&B> for &mut A {
    fn eq(&self, other: &&B) -> bool {
        PartialEq::eq(&**self, &**other)
    }
    fn ne(&self, other: &&B) -> bool {
        PartialEq::ne(&**self, &**other)
    }
}

impl<A: Eq> Eq for &A {}
impl<A: Eq> Eq for &mut A {}

/// The result of comparing two values: `1.cmp(&2)` is `Less`.
#[lang = "Ordering"]
#[repr(i8)]
#[derive(Debug)]
pub enum Ordering {
    Less = -1,
    Equal = 0,
    Greater = 1,
}

impl Ordering {
    /// Whether it is `Equal`.
    pub const fn is_eq(self) -> bool {
        match self {
            Equal => true,
            _ => false,
        }
    }

    /// Whether it is not `Equal`.
    pub const fn is_ne(self) -> bool {
        !self.is_eq()
    }

    /// Whether it is `Less`.
    pub const fn is_lt(self) -> bool {
        match self {
            Less => true,
            _ => false,
        }
    }

    /// Whether it is `Greater`.
    pub const fn is_gt(self) -> bool {
        match self {
            Greater => true,
            _ => false,
        }
    }

    /// Whether it is `Less` or `Equal`.
    pub const fn is_le(self) -> bool {
        !self.is_gt()
    }

    /// Whether it is `Greater` or `Equal`.
    pub const fn is_ge(self) -> bool {
        !self.is_lt()
    }

    /// `Less` for `Greater` and the reverse; `Equal` stays.
    pub const fn reverse(self) -> Ordering {
        match self {
            Less => Greater,
            Equal => Equal,
            Greater => Less,
        }
    }

    /// Itself, unless it is `Equal`, then `other`: how to compare by one
    /// key, then another.
    pub const fn then(self, other: Ordering) -> Ordering {
        match self {
            Equal => other,
            _ => self,
        }
    }

    /// Itself, or `f()` if it is `Equal`.
    pub fn then_with<F: crate::ops::FnOnce() -> Ordering>(self, f: F) -> Ordering {
        match self {
            Equal => f(),
            _ => self,
        }
    }
}

impl crate::clone::Clone for Ordering {
    fn clone(&self) -> Ordering {
        *self
    }
}

impl crate::marker::Copy for Ordering {}

impl PartialEq for Ordering {
    fn eq(&self, other: &Ordering) -> bool {
        (*self as i8) == (*other as i8)
    }
}

impl Eq for Ordering {}

impl PartialOrd for Ordering {
    fn partial_cmp(&self, other: &Ordering) -> Option<Ordering> {
        (*self as i8).partial_cmp(&(*other as i8))
    }
}

impl Ord for Ordering {
    fn cmp(&self, other: &Ordering) -> Ordering {
        (*self as i8).cmp(&(*other as i8))
    }
}

/// The `<`, `<=`, `>` and `>=` operators, for values that may be
/// unordered: `partial_cmp` gives `None` for those.
#[lang = "partial_ord"]
#[diagnostic::on_unimplemented(message = "can't compare `{Self}` with `{Rhs}`")]
pub trait PartialOrd<Rhs = Self>: PartialEq<Rhs> {
    /// How `self` compares with `other`, if they are ordered.
    fn partial_cmp(&self, other: &Rhs) -> Option<Ordering>;

    /// `self < other`.
    fn lt(&self, other: &Rhs) -> bool {
        match self.partial_cmp(other) {
            Some(Less) => true,
            _ => false,
        }
    }

    /// `self <= other`.
    fn le(&self, other: &Rhs) -> bool {
        match self.partial_cmp(other) {
            Some(Less | Equal) => true,
            _ => false,
        }
    }

    /// `self > other`.
    fn gt(&self, other: &Rhs) -> bool {
        match self.partial_cmp(other) {
            Some(Greater) => true,
            _ => false,
        }
    }

    /// `self >= other`.
    fn ge(&self, other: &Rhs) -> bool {
        match self.partial_cmp(other) {
            Some(Greater | Equal) => true,
            _ => false,
        }
    }
}

/// A total order: any two values are ordered.
#[lang = "Ord"]
pub trait Ord: Eq + PartialOrd {
    /// How `self` compares with `other`.
    fn cmp(&self, other: &Self) -> Ordering;

    /// The greater of the two, `other` if they are equal.
    fn max(self, other: Self) -> Self
    where
        Self: Sized,
    {
        if other < self { self } else { other }
    }

    /// The lesser of the two, `self` if they are equal.
    fn min(self, other: Self) -> Self
    where
        Self: Sized,
    {
        if other < self { other } else { self }
    }

    /// `min` if `self` is less, `max` if it is greater, else `self`;
    /// panics if `min > max`.
    fn clamp(self, min: Self, max: Self) -> Self
    where
        Self: Sized,
    {
        if min > max {
            panic(c"assertion failed: min <= max");
        }
        if self < min {
            min
        } else if self > max {
            max
        } else {
            self
        }
    }
}

/// The greater of `v1` and `v2`, `v2` if they are equal.
#[rustc_const_unstable]
pub fn max<T: Ord>(v1: T, v2: T) -> T {
    v1.max(v2)
}

/// The lesser of `v1` and `v2`, `v1` if they are equal.
#[rustc_const_unstable]
pub fn min<T: Ord>(v1: T, v2: T) -> T {
    v1.min(v2)
}

/// The greater of `v1` and `v2` by `compare`, `v2` if they are equal.
pub fn max_by<T, F: crate::ops::FnOnce(&T, &T) -> Ordering>(v1: T, v2: T, compare: F) -> T {
    match compare(&v1, &v2) {
        Greater => v1,
        _ => v2,
    }
}

/// The lesser of `v1` and `v2` by `compare`, `v1` if they are equal.
pub fn min_by<T, F: crate::ops::FnOnce(&T, &T) -> Ordering>(v1: T, v2: T, compare: F) -> T {
    match compare(&v1, &v2) {
        Greater => v2,
        _ => v1,
    }
}

/// The greater of `v1` and `v2` by `f`'s key, `v2` if they are equal.
pub fn max_by_key<T, F: crate::ops::FnMut(&T) -> K, K: Ord>(v1: T, v2: T, mut f: F) -> T {
    let (k1, k2) = (f(&v1), f(&v2));
    if k1 > k2 { v1 } else { v2 }
}

/// The lesser of `v1` and `v2` by `f`'s key, `v1` if they are equal.
pub fn min_by_key<T, F: crate::ops::FnMut(&T) -> K, K: Ord>(v1: T, v2: T, mut f: F) -> T {
    let (k1, k2) = (f(&v1), f(&v2));
    if k2 < k1 { v2 } else { v1 }
}

// The integers and `bool` compare with the built-in operators.
impl PartialOrd for i8 { fn partial_cmp(&self, other: &i8) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &i8) -> bool { *self < *other } fn le(&self, other: &i8) -> bool { *self <= *other } fn gt(&self, other: &i8) -> bool { *self > *other } fn ge(&self, other: &i8) -> bool { *self >= *other } }
impl Ord for i8 { fn cmp(&self, other: &i8) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } } }
impl PartialOrd for i16 { fn partial_cmp(&self, other: &i16) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &i16) -> bool { *self < *other } fn le(&self, other: &i16) -> bool { *self <= *other } fn gt(&self, other: &i16) -> bool { *self > *other } fn ge(&self, other: &i16) -> bool { *self >= *other } }
impl Ord for i16 { fn cmp(&self, other: &i16) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } } }
impl PartialOrd for i32 { fn partial_cmp(&self, other: &i32) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &i32) -> bool { *self < *other } fn le(&self, other: &i32) -> bool { *self <= *other } fn gt(&self, other: &i32) -> bool { *self > *other } fn ge(&self, other: &i32) -> bool { *self >= *other } }
impl Ord for i32 { fn cmp(&self, other: &i32) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } } }
impl PartialOrd for i64 { fn partial_cmp(&self, other: &i64) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &i64) -> bool { *self < *other } fn le(&self, other: &i64) -> bool { *self <= *other } fn gt(&self, other: &i64) -> bool { *self > *other } fn ge(&self, other: &i64) -> bool { *self >= *other } }
impl Ord for i64 { fn cmp(&self, other: &i64) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } } }
impl PartialOrd for i128 { fn partial_cmp(&self, other: &i128) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &i128) -> bool { *self < *other } fn le(&self, other: &i128) -> bool { *self <= *other } fn gt(&self, other: &i128) -> bool { *self > *other } fn ge(&self, other: &i128) -> bool { *self >= *other } }
impl Ord for i128 { fn cmp(&self, other: &i128) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } } }
impl PartialOrd for isize { fn partial_cmp(&self, other: &isize) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &isize) -> bool { *self < *other } fn le(&self, other: &isize) -> bool { *self <= *other } fn gt(&self, other: &isize) -> bool { *self > *other } fn ge(&self, other: &isize) -> bool { *self >= *other } }
impl Ord for isize { fn cmp(&self, other: &isize) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } } }
impl PartialOrd for u8 { fn partial_cmp(&self, other: &u8) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &u8) -> bool { *self < *other } fn le(&self, other: &u8) -> bool { *self <= *other } fn gt(&self, other: &u8) -> bool { *self > *other } fn ge(&self, other: &u8) -> bool { *self >= *other } }
impl Ord for u8 { fn cmp(&self, other: &u8) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } } }
impl PartialOrd for u16 { fn partial_cmp(&self, other: &u16) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &u16) -> bool { *self < *other } fn le(&self, other: &u16) -> bool { *self <= *other } fn gt(&self, other: &u16) -> bool { *self > *other } fn ge(&self, other: &u16) -> bool { *self >= *other } }
impl Ord for u16 { fn cmp(&self, other: &u16) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } } }
impl PartialOrd for u32 { fn partial_cmp(&self, other: &u32) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &u32) -> bool { *self < *other } fn le(&self, other: &u32) -> bool { *self <= *other } fn gt(&self, other: &u32) -> bool { *self > *other } fn ge(&self, other: &u32) -> bool { *self >= *other } }
impl Ord for u32 { fn cmp(&self, other: &u32) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } } }
impl PartialOrd for u64 { fn partial_cmp(&self, other: &u64) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &u64) -> bool { *self < *other } fn le(&self, other: &u64) -> bool { *self <= *other } fn gt(&self, other: &u64) -> bool { *self > *other } fn ge(&self, other: &u64) -> bool { *self >= *other } }
impl Ord for u64 { fn cmp(&self, other: &u64) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } } }
impl PartialOrd for u128 { fn partial_cmp(&self, other: &u128) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &u128) -> bool { *self < *other } fn le(&self, other: &u128) -> bool { *self <= *other } fn gt(&self, other: &u128) -> bool { *self > *other } fn ge(&self, other: &u128) -> bool { *self >= *other } }
impl Ord for u128 { fn cmp(&self, other: &u128) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } } }
impl PartialOrd for usize { fn partial_cmp(&self, other: &usize) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &usize) -> bool { *self < *other } fn le(&self, other: &usize) -> bool { *self <= *other } fn gt(&self, other: &usize) -> bool { *self > *other } fn ge(&self, other: &usize) -> bool { *self >= *other } }
impl Ord for usize { fn cmp(&self, other: &usize) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } } }
impl PartialOrd for bool { fn partial_cmp(&self, other: &bool) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &bool) -> bool { *self < *other } fn le(&self, other: &bool) -> bool { *self <= *other } fn gt(&self, other: &bool) -> bool { *self > *other } fn ge(&self, other: &bool) -> bool { *self >= *other } }
impl Ord for bool { fn cmp(&self, other: &bool) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } } }
impl PartialOrd for char { fn partial_cmp(&self, other: &char) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &char) -> bool { *self < *other } fn le(&self, other: &char) -> bool { *self <= *other } fn gt(&self, other: &char) -> bool { *self > *other } fn ge(&self, other: &char) -> bool { *self >= *other } }
impl Ord for char { fn cmp(&self, other: &char) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } } }
impl PartialOrd for () { fn partial_cmp(&self, _other: &()) -> Option<Ordering> { Some(Equal) } }
impl Ord for () { fn cmp(&self, _other: &()) -> Ordering { Equal } }

// References compare what they point to, as in rustc: `&a < &b` is
// `a < b`. Unlike `PartialEq`, a `&` compares only with a `&`.
impl<A: PartialOrd<B>, B> PartialOrd<&B> for &A {
    fn partial_cmp(&self, other: &&B) -> Option<Ordering> {
        PartialOrd::partial_cmp(*self, *other)
    }
    fn lt(&self, other: &&B) -> bool {
        PartialOrd::lt(*self, *other)
    }
    fn le(&self, other: &&B) -> bool {
        PartialOrd::le(*self, *other)
    }
    fn gt(&self, other: &&B) -> bool {
        PartialOrd::gt(*self, *other)
    }
    fn ge(&self, other: &&B) -> bool {
        PartialOrd::ge(*self, *other)
    }
}

impl<A: PartialOrd<B>, B> PartialOrd<&mut B> for &mut A {
    fn partial_cmp(&self, other: &&mut B) -> Option<Ordering> {
        PartialOrd::partial_cmp(&**self, &**other)
    }
    fn lt(&self, other: &&mut B) -> bool {
        PartialOrd::lt(&**self, &**other)
    }
    fn le(&self, other: &&mut B) -> bool {
        PartialOrd::le(&**self, &**other)
    }
    fn gt(&self, other: &&mut B) -> bool {
        PartialOrd::gt(&**self, &**other)
    }
    fn ge(&self, other: &&mut B) -> bool {
        PartialOrd::ge(&**self, &**other)
    }
}

impl<A: Ord> Ord for &A {
    fn cmp(&self, other: &Self) -> Ordering {
        Ord::cmp(*self, *other)
    }
}

impl<A: Ord> Ord for &mut A {
    fn cmp(&self, other: &Self) -> Ordering {
        Ord::cmp(&**self, &**other)
    }
}
