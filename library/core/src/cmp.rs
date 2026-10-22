//! Comparison: `a == b` and `a != b` on types other than integers and
//! `bool` are `PartialEq::eq(&a, &b)` and `PartialEq::ne(&a, &b)`, and
//! `a < b` is `PartialOrd::lt(&a, &b)` (`le`, `gt` and `ge` for `<=`, `>`
//! and `>=`).

use crate::clone::Clone;
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
#[derive(Debug, Hash)]
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
            panic("assertion failed: min <= max");
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

/// A value that orders the other way round: `Reverse(a) < Reverse(b)`
/// where `b < a`, to sort descending by a key or make a max-heap a
/// min-heap.
#[derive(PartialEq, Eq, Debug, Copy, Default, Hash)]
pub struct Reverse<T>(pub T);

impl<T: PartialOrd> PartialOrd for Reverse<T> {
    fn partial_cmp(&self, other: &Reverse<T>) -> Option<Ordering> {
        other.0.partial_cmp(&self.0)
    }

    fn lt(&self, other: &Self) -> bool {
        other.0 < self.0
    }

    fn le(&self, other: &Self) -> bool {
        other.0 <= self.0
    }

    fn gt(&self, other: &Self) -> bool {
        other.0 > self.0
    }

    fn ge(&self, other: &Self) -> bool {
        other.0 >= self.0
    }
}

impl<T: Ord> Ord for Reverse<T> {
    fn cmp(&self, other: &Reverse<T>) -> Ordering {
        other.0.cmp(&self.0)
    }
}

impl<T: Clone> Clone for Reverse<T> {
    fn clone(&self) -> Reverse<T> {
        Reverse(self.0.clone())
    }
}

// The integers and `bool` compare with the built-in operators.
impl PartialOrd for i8 { fn partial_cmp(&self, other: &i8) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &i8) -> bool { *self < *other } fn le(&self, other: &i8) -> bool { *self <= *other } fn gt(&self, other: &i8) -> bool { *self > *other } fn ge(&self, other: &i8) -> bool { *self >= *other } }
impl Ord for i8 {
    fn cmp(&self, other: &i8) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } }

    /// As rustc's, which says where it was called.
    #[track_caller]
    fn clamp(self, min: i8, max: i8) -> i8 {
        if min > max {
            crate::panicking::panic_fmt(format_args!("min > max. min = {min:?}, max = {max:?}"))
        }
        if self < min { min } else if self > max { max } else { self }
    }
}
impl PartialOrd for i16 { fn partial_cmp(&self, other: &i16) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &i16) -> bool { *self < *other } fn le(&self, other: &i16) -> bool { *self <= *other } fn gt(&self, other: &i16) -> bool { *self > *other } fn ge(&self, other: &i16) -> bool { *self >= *other } }
impl Ord for i16 {
    fn cmp(&self, other: &i16) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } }

    /// As rustc's, which says where it was called.
    #[track_caller]
    fn clamp(self, min: i16, max: i16) -> i16 {
        if min > max {
            crate::panicking::panic_fmt(format_args!("min > max. min = {min:?}, max = {max:?}"))
        }
        if self < min { min } else if self > max { max } else { self }
    }
}
impl PartialOrd for i32 { fn partial_cmp(&self, other: &i32) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &i32) -> bool { *self < *other } fn le(&self, other: &i32) -> bool { *self <= *other } fn gt(&self, other: &i32) -> bool { *self > *other } fn ge(&self, other: &i32) -> bool { *self >= *other } }
impl Ord for i32 {
    fn cmp(&self, other: &i32) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } }

    /// As rustc's, which says where it was called.
    #[track_caller]
    fn clamp(self, min: i32, max: i32) -> i32 {
        if min > max {
            crate::panicking::panic_fmt(format_args!("min > max. min = {min:?}, max = {max:?}"))
        }
        if self < min { min } else if self > max { max } else { self }
    }
}
impl PartialOrd for i64 { fn partial_cmp(&self, other: &i64) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &i64) -> bool { *self < *other } fn le(&self, other: &i64) -> bool { *self <= *other } fn gt(&self, other: &i64) -> bool { *self > *other } fn ge(&self, other: &i64) -> bool { *self >= *other } }
impl Ord for i64 {
    fn cmp(&self, other: &i64) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } }

    /// As rustc's, which says where it was called.
    #[track_caller]
    fn clamp(self, min: i64, max: i64) -> i64 {
        if min > max {
            crate::panicking::panic_fmt(format_args!("min > max. min = {min:?}, max = {max:?}"))
        }
        if self < min { min } else if self > max { max } else { self }
    }
}
impl PartialOrd for i128 { fn partial_cmp(&self, other: &i128) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &i128) -> bool { *self < *other } fn le(&self, other: &i128) -> bool { *self <= *other } fn gt(&self, other: &i128) -> bool { *self > *other } fn ge(&self, other: &i128) -> bool { *self >= *other } }
impl Ord for i128 {
    fn cmp(&self, other: &i128) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } }

    /// As rustc's, which says where it was called.
    #[track_caller]
    fn clamp(self, min: i128, max: i128) -> i128 {
        if min > max {
            crate::panicking::panic_fmt(format_args!("min > max. min = {min:?}, max = {max:?}"))
        }
        if self < min { min } else if self > max { max } else { self }
    }
}
impl PartialOrd for isize { fn partial_cmp(&self, other: &isize) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &isize) -> bool { *self < *other } fn le(&self, other: &isize) -> bool { *self <= *other } fn gt(&self, other: &isize) -> bool { *self > *other } fn ge(&self, other: &isize) -> bool { *self >= *other } }
impl Ord for isize {
    fn cmp(&self, other: &isize) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } }

    /// As rustc's, which says where it was called.
    #[track_caller]
    fn clamp(self, min: isize, max: isize) -> isize {
        if min > max {
            crate::panicking::panic_fmt(format_args!("min > max. min = {min:?}, max = {max:?}"))
        }
        if self < min { min } else if self > max { max } else { self }
    }
}
impl PartialOrd for u8 { fn partial_cmp(&self, other: &u8) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &u8) -> bool { *self < *other } fn le(&self, other: &u8) -> bool { *self <= *other } fn gt(&self, other: &u8) -> bool { *self > *other } fn ge(&self, other: &u8) -> bool { *self >= *other } }
impl Ord for u8 {
    fn cmp(&self, other: &u8) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } }

    /// As rustc's, which says where it was called.
    #[track_caller]
    fn clamp(self, min: u8, max: u8) -> u8 {
        if min > max {
            crate::panicking::panic_fmt(format_args!("min > max. min = {min:?}, max = {max:?}"))
        }
        if self < min { min } else if self > max { max } else { self }
    }
}
impl PartialOrd for u16 { fn partial_cmp(&self, other: &u16) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &u16) -> bool { *self < *other } fn le(&self, other: &u16) -> bool { *self <= *other } fn gt(&self, other: &u16) -> bool { *self > *other } fn ge(&self, other: &u16) -> bool { *self >= *other } }
impl Ord for u16 {
    fn cmp(&self, other: &u16) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } }

    /// As rustc's, which says where it was called.
    #[track_caller]
    fn clamp(self, min: u16, max: u16) -> u16 {
        if min > max {
            crate::panicking::panic_fmt(format_args!("min > max. min = {min:?}, max = {max:?}"))
        }
        if self < min { min } else if self > max { max } else { self }
    }
}
impl PartialOrd for u32 { fn partial_cmp(&self, other: &u32) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &u32) -> bool { *self < *other } fn le(&self, other: &u32) -> bool { *self <= *other } fn gt(&self, other: &u32) -> bool { *self > *other } fn ge(&self, other: &u32) -> bool { *self >= *other } }
impl Ord for u32 {
    fn cmp(&self, other: &u32) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } }

    /// As rustc's, which says where it was called.
    #[track_caller]
    fn clamp(self, min: u32, max: u32) -> u32 {
        if min > max {
            crate::panicking::panic_fmt(format_args!("min > max. min = {min:?}, max = {max:?}"))
        }
        if self < min { min } else if self > max { max } else { self }
    }
}
impl PartialOrd for u64 { fn partial_cmp(&self, other: &u64) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &u64) -> bool { *self < *other } fn le(&self, other: &u64) -> bool { *self <= *other } fn gt(&self, other: &u64) -> bool { *self > *other } fn ge(&self, other: &u64) -> bool { *self >= *other } }
impl Ord for u64 {
    fn cmp(&self, other: &u64) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } }

    /// As rustc's, which says where it was called.
    #[track_caller]
    fn clamp(self, min: u64, max: u64) -> u64 {
        if min > max {
            crate::panicking::panic_fmt(format_args!("min > max. min = {min:?}, max = {max:?}"))
        }
        if self < min { min } else if self > max { max } else { self }
    }
}
impl PartialOrd for u128 { fn partial_cmp(&self, other: &u128) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &u128) -> bool { *self < *other } fn le(&self, other: &u128) -> bool { *self <= *other } fn gt(&self, other: &u128) -> bool { *self > *other } fn ge(&self, other: &u128) -> bool { *self >= *other } }
impl Ord for u128 {
    fn cmp(&self, other: &u128) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } }

    /// As rustc's, which says where it was called.
    #[track_caller]
    fn clamp(self, min: u128, max: u128) -> u128 {
        if min > max {
            crate::panicking::panic_fmt(format_args!("min > max. min = {min:?}, max = {max:?}"))
        }
        if self < min { min } else if self > max { max } else { self }
    }
}
impl PartialOrd for usize { fn partial_cmp(&self, other: &usize) -> Option<Ordering> { Some(self.cmp(other)) } fn lt(&self, other: &usize) -> bool { *self < *other } fn le(&self, other: &usize) -> bool { *self <= *other } fn gt(&self, other: &usize) -> bool { *self > *other } fn ge(&self, other: &usize) -> bool { *self >= *other } }
impl Ord for usize {
    fn cmp(&self, other: &usize) -> Ordering { if *self < *other { Less } else if *self == *other { Equal } else { Greater } }

    /// As rustc's, which says where it was called.
    #[track_caller]
    fn clamp(self, min: usize, max: usize) -> usize {
        if min > max {
            crate::panicking::panic_fmt(format_args!("min > max. min = {min:?}, max = {max:?}"))
        }
        if self < min { min } else if self > max { max } else { self }
    }
}
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

// Tuples of up to twelve elements compare element by element, the first
// unequal pair deciding, as rustc's `core` has them: `ne` with `!=` of
// each pair, and `lt` and the others through each pair's `partial_cmp`
// but the last's operator.
impl<A: PartialEq> PartialEq for (A,) {
    fn eq(&self, other: &(A,)) -> bool {
        self.0 == other.0
    }
    fn ne(&self, other: &(A,)) -> bool {
        self.0 != other.0
    }
}
impl<A: Eq> Eq for (A,) {}
impl<A: PartialOrd> PartialOrd for (A,) {
    fn partial_cmp(&self, other: &(A,)) -> Option<Ordering> {
        PartialOrd::partial_cmp(&self.0, &other.0)
    }

    fn lt(&self, other: &(A,)) -> bool {
        self.0 < other.0
    }

    fn le(&self, other: &(A,)) -> bool {
        self.0 <= other.0
    }

    fn gt(&self, other: &(A,)) -> bool {
        self.0 > other.0
    }

    fn ge(&self, other: &(A,)) -> bool {
        self.0 >= other.0
    }
}
impl<A: Ord> Ord for (A,) {
    fn cmp(&self, other: &(A,)) -> Ordering {
        Ord::cmp(&self.0, &other.0)
    }
}
impl<A: PartialEq, B: PartialEq> PartialEq for (A, B) {
    fn eq(&self, other: &(A, B)) -> bool {
        self.0 == other.0 && self.1 == other.1
    }
    fn ne(&self, other: &(A, B)) -> bool {
        self.0 != other.0 || self.1 != other.1
    }
}
impl<A: Eq, B: Eq> Eq for (A, B) {}
impl<A: PartialOrd, B: PartialOrd> PartialOrd for (A, B) {
    fn partial_cmp(&self, other: &(A, B)) -> Option<Ordering> {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        PartialOrd::partial_cmp(&self.1, &other.1)
    }

    fn lt(&self, other: &(A, B)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.1 < other.1
    }

    fn le(&self, other: &(A, B)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.1 <= other.1
    }

    fn gt(&self, other: &(A, B)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.1 > other.1
    }

    fn ge(&self, other: &(A, B)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.1 >= other.1
    }
}
impl<A: Ord, B: Ord> Ord for (A, B) {
    fn cmp(&self, other: &(A, B)) -> Ordering {
        match Ord::cmp(&self.0, &other.0) {
            Equal => {}
            ordering => return ordering,
        }
        Ord::cmp(&self.1, &other.1)
    }
}
impl<A: PartialEq, B: PartialEq, C: PartialEq> PartialEq for (A, B, C) {
    fn eq(&self, other: &(A, B, C)) -> bool {
        self.0 == other.0 && self.1 == other.1 && self.2 == other.2
    }
    fn ne(&self, other: &(A, B, C)) -> bool {
        self.0 != other.0 || self.1 != other.1 || self.2 != other.2
    }
}
impl<A: Eq, B: Eq, C: Eq> Eq for (A, B, C) {}
impl<A: PartialOrd, B: PartialOrd, C: PartialOrd> PartialOrd for (A, B, C) {
    fn partial_cmp(&self, other: &(A, B, C)) -> Option<Ordering> {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        PartialOrd::partial_cmp(&self.2, &other.2)
    }

    fn lt(&self, other: &(A, B, C)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.2 < other.2
    }

    fn le(&self, other: &(A, B, C)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.2 <= other.2
    }

    fn gt(&self, other: &(A, B, C)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.2 > other.2
    }

    fn ge(&self, other: &(A, B, C)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.2 >= other.2
    }
}
impl<A: Ord, B: Ord, C: Ord> Ord for (A, B, C) {
    fn cmp(&self, other: &(A, B, C)) -> Ordering {
        match Ord::cmp(&self.0, &other.0) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.1, &other.1) {
            Equal => {}
            ordering => return ordering,
        }
        Ord::cmp(&self.2, &other.2)
    }
}
impl<A: PartialEq, B: PartialEq, C: PartialEq, D: PartialEq> PartialEq for (A, B, C, D) {
    fn eq(&self, other: &(A, B, C, D)) -> bool {
        self.0 == other.0 && self.1 == other.1 && self.2 == other.2 && self.3 == other.3
    }
    fn ne(&self, other: &(A, B, C, D)) -> bool {
        self.0 != other.0 || self.1 != other.1 || self.2 != other.2 || self.3 != other.3
    }
}
impl<A: Eq, B: Eq, C: Eq, D: Eq> Eq for (A, B, C, D) {}
impl<A: PartialOrd, B: PartialOrd, C: PartialOrd, D: PartialOrd> PartialOrd for (A, B, C, D) {
    fn partial_cmp(&self, other: &(A, B, C, D)) -> Option<Ordering> {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        PartialOrd::partial_cmp(&self.3, &other.3)
    }

    fn lt(&self, other: &(A, B, C, D)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.3 < other.3
    }

    fn le(&self, other: &(A, B, C, D)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.3 <= other.3
    }

    fn gt(&self, other: &(A, B, C, D)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.3 > other.3
    }

    fn ge(&self, other: &(A, B, C, D)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.3 >= other.3
    }
}
impl<A: Ord, B: Ord, C: Ord, D: Ord> Ord for (A, B, C, D) {
    fn cmp(&self, other: &(A, B, C, D)) -> Ordering {
        match Ord::cmp(&self.0, &other.0) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.1, &other.1) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.2, &other.2) {
            Equal => {}
            ordering => return ordering,
        }
        Ord::cmp(&self.3, &other.3)
    }
}
impl<A: PartialEq, B: PartialEq, C: PartialEq, D: PartialEq, E: PartialEq> PartialEq for (A, B, C, D, E) {
    fn eq(&self, other: &(A, B, C, D, E)) -> bool {
        self.0 == other.0 && self.1 == other.1 && self.2 == other.2 && self.3 == other.3 && self.4 == other.4
    }
    fn ne(&self, other: &(A, B, C, D, E)) -> bool {
        self.0 != other.0 || self.1 != other.1 || self.2 != other.2 || self.3 != other.3 || self.4 != other.4
    }
}
impl<A: Eq, B: Eq, C: Eq, D: Eq, E: Eq> Eq for (A, B, C, D, E) {}
impl<A: PartialOrd, B: PartialOrd, C: PartialOrd, D: PartialOrd, E: PartialOrd> PartialOrd for (A, B, C, D, E) {
    fn partial_cmp(&self, other: &(A, B, C, D, E)) -> Option<Ordering> {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        PartialOrd::partial_cmp(&self.4, &other.4)
    }

    fn lt(&self, other: &(A, B, C, D, E)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.4 < other.4
    }

    fn le(&self, other: &(A, B, C, D, E)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.4 <= other.4
    }

    fn gt(&self, other: &(A, B, C, D, E)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.4 > other.4
    }

    fn ge(&self, other: &(A, B, C, D, E)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.4 >= other.4
    }
}
impl<A: Ord, B: Ord, C: Ord, D: Ord, E: Ord> Ord for (A, B, C, D, E) {
    fn cmp(&self, other: &(A, B, C, D, E)) -> Ordering {
        match Ord::cmp(&self.0, &other.0) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.1, &other.1) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.2, &other.2) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.3, &other.3) {
            Equal => {}
            ordering => return ordering,
        }
        Ord::cmp(&self.4, &other.4)
    }
}
impl<A: PartialEq, B: PartialEq, C: PartialEq, D: PartialEq, E: PartialEq, F: PartialEq> PartialEq for (A, B, C, D, E, F) {
    fn eq(&self, other: &(A, B, C, D, E, F)) -> bool {
        self.0 == other.0 && self.1 == other.1 && self.2 == other.2 && self.3 == other.3 && self.4 == other.4 && self.5 == other.5
    }
    fn ne(&self, other: &(A, B, C, D, E, F)) -> bool {
        self.0 != other.0 || self.1 != other.1 || self.2 != other.2 || self.3 != other.3 || self.4 != other.4 || self.5 != other.5
    }
}
impl<A: Eq, B: Eq, C: Eq, D: Eq, E: Eq, F: Eq> Eq for (A, B, C, D, E, F) {}
impl<A: PartialOrd, B: PartialOrd, C: PartialOrd, D: PartialOrd, E: PartialOrd, F: PartialOrd> PartialOrd for (A, B, C, D, E, F) {
    fn partial_cmp(&self, other: &(A, B, C, D, E, F)) -> Option<Ordering> {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        PartialOrd::partial_cmp(&self.5, &other.5)
    }

    fn lt(&self, other: &(A, B, C, D, E, F)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.5 < other.5
    }

    fn le(&self, other: &(A, B, C, D, E, F)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.5 <= other.5
    }

    fn gt(&self, other: &(A, B, C, D, E, F)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.5 > other.5
    }

    fn ge(&self, other: &(A, B, C, D, E, F)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.5 >= other.5
    }
}
impl<A: Ord, B: Ord, C: Ord, D: Ord, E: Ord, F: Ord> Ord for (A, B, C, D, E, F) {
    fn cmp(&self, other: &(A, B, C, D, E, F)) -> Ordering {
        match Ord::cmp(&self.0, &other.0) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.1, &other.1) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.2, &other.2) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.3, &other.3) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.4, &other.4) {
            Equal => {}
            ordering => return ordering,
        }
        Ord::cmp(&self.5, &other.5)
    }
}
impl<A: PartialEq, B: PartialEq, C: PartialEq, D: PartialEq, E: PartialEq, F: PartialEq, G: PartialEq> PartialEq for (A, B, C, D, E, F, G) {
    fn eq(&self, other: &(A, B, C, D, E, F, G)) -> bool {
        self.0 == other.0 && self.1 == other.1 && self.2 == other.2 && self.3 == other.3 && self.4 == other.4 && self.5 == other.5 && self.6 == other.6
    }
    fn ne(&self, other: &(A, B, C, D, E, F, G)) -> bool {
        self.0 != other.0 || self.1 != other.1 || self.2 != other.2 || self.3 != other.3 || self.4 != other.4 || self.5 != other.5 || self.6 != other.6
    }
}
impl<A: Eq, B: Eq, C: Eq, D: Eq, E: Eq, F: Eq, G: Eq> Eq for (A, B, C, D, E, F, G) {}
impl<A: PartialOrd, B: PartialOrd, C: PartialOrd, D: PartialOrd, E: PartialOrd, F: PartialOrd, G: PartialOrd> PartialOrd for (A, B, C, D, E, F, G) {
    fn partial_cmp(&self, other: &(A, B, C, D, E, F, G)) -> Option<Ordering> {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        PartialOrd::partial_cmp(&self.6, &other.6)
    }

    fn lt(&self, other: &(A, B, C, D, E, F, G)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.6 < other.6
    }

    fn le(&self, other: &(A, B, C, D, E, F, G)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.6 <= other.6
    }

    fn gt(&self, other: &(A, B, C, D, E, F, G)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.6 > other.6
    }

    fn ge(&self, other: &(A, B, C, D, E, F, G)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.6 >= other.6
    }
}
impl<A: Ord, B: Ord, C: Ord, D: Ord, E: Ord, F: Ord, G: Ord> Ord for (A, B, C, D, E, F, G) {
    fn cmp(&self, other: &(A, B, C, D, E, F, G)) -> Ordering {
        match Ord::cmp(&self.0, &other.0) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.1, &other.1) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.2, &other.2) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.3, &other.3) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.4, &other.4) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.5, &other.5) {
            Equal => {}
            ordering => return ordering,
        }
        Ord::cmp(&self.6, &other.6)
    }
}
impl<A: PartialEq, B: PartialEq, C: PartialEq, D: PartialEq, E: PartialEq, F: PartialEq, G: PartialEq, H: PartialEq> PartialEq for (A, B, C, D, E, F, G, H) {
    fn eq(&self, other: &(A, B, C, D, E, F, G, H)) -> bool {
        self.0 == other.0 && self.1 == other.1 && self.2 == other.2 && self.3 == other.3 && self.4 == other.4 && self.5 == other.5 && self.6 == other.6 && self.7 == other.7
    }
    fn ne(&self, other: &(A, B, C, D, E, F, G, H)) -> bool {
        self.0 != other.0 || self.1 != other.1 || self.2 != other.2 || self.3 != other.3 || self.4 != other.4 || self.5 != other.5 || self.6 != other.6 || self.7 != other.7
    }
}
impl<A: Eq, B: Eq, C: Eq, D: Eq, E: Eq, F: Eq, G: Eq, H: Eq> Eq for (A, B, C, D, E, F, G, H) {}
impl<A: PartialOrd, B: PartialOrd, C: PartialOrd, D: PartialOrd, E: PartialOrd, F: PartialOrd, G: PartialOrd, H: PartialOrd> PartialOrd for (A, B, C, D, E, F, G, H) {
    fn partial_cmp(&self, other: &(A, B, C, D, E, F, G, H)) -> Option<Ordering> {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        PartialOrd::partial_cmp(&self.7, &other.7)
    }

    fn lt(&self, other: &(A, B, C, D, E, F, G, H)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.7 < other.7
    }

    fn le(&self, other: &(A, B, C, D, E, F, G, H)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.7 <= other.7
    }

    fn gt(&self, other: &(A, B, C, D, E, F, G, H)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.7 > other.7
    }

    fn ge(&self, other: &(A, B, C, D, E, F, G, H)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.7 >= other.7
    }
}
impl<A: Ord, B: Ord, C: Ord, D: Ord, E: Ord, F: Ord, G: Ord, H: Ord> Ord for (A, B, C, D, E, F, G, H) {
    fn cmp(&self, other: &(A, B, C, D, E, F, G, H)) -> Ordering {
        match Ord::cmp(&self.0, &other.0) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.1, &other.1) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.2, &other.2) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.3, &other.3) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.4, &other.4) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.5, &other.5) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.6, &other.6) {
            Equal => {}
            ordering => return ordering,
        }
        Ord::cmp(&self.7, &other.7)
    }
}
impl<A: PartialEq, B: PartialEq, C: PartialEq, D: PartialEq, E: PartialEq, F: PartialEq, G: PartialEq, H: PartialEq, I: PartialEq> PartialEq for (A, B, C, D, E, F, G, H, I) {
    fn eq(&self, other: &(A, B, C, D, E, F, G, H, I)) -> bool {
        self.0 == other.0 && self.1 == other.1 && self.2 == other.2 && self.3 == other.3 && self.4 == other.4 && self.5 == other.5 && self.6 == other.6 && self.7 == other.7 && self.8 == other.8
    }
    fn ne(&self, other: &(A, B, C, D, E, F, G, H, I)) -> bool {
        self.0 != other.0 || self.1 != other.1 || self.2 != other.2 || self.3 != other.3 || self.4 != other.4 || self.5 != other.5 || self.6 != other.6 || self.7 != other.7 || self.8 != other.8
    }
}
impl<A: Eq, B: Eq, C: Eq, D: Eq, E: Eq, F: Eq, G: Eq, H: Eq, I: Eq> Eq for (A, B, C, D, E, F, G, H, I) {}
impl<A: PartialOrd, B: PartialOrd, C: PartialOrd, D: PartialOrd, E: PartialOrd, F: PartialOrd, G: PartialOrd, H: PartialOrd, I: PartialOrd> PartialOrd for (A, B, C, D, E, F, G, H, I) {
    fn partial_cmp(&self, other: &(A, B, C, D, E, F, G, H, I)) -> Option<Ordering> {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.7, &other.7) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        PartialOrd::partial_cmp(&self.8, &other.8)
    }

    fn lt(&self, other: &(A, B, C, D, E, F, G, H, I)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.7, &other.7) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.8 < other.8
    }

    fn le(&self, other: &(A, B, C, D, E, F, G, H, I)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.7, &other.7) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.8 <= other.8
    }

    fn gt(&self, other: &(A, B, C, D, E, F, G, H, I)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.7, &other.7) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.8 > other.8
    }

    fn ge(&self, other: &(A, B, C, D, E, F, G, H, I)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.7, &other.7) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.8 >= other.8
    }
}
impl<A: Ord, B: Ord, C: Ord, D: Ord, E: Ord, F: Ord, G: Ord, H: Ord, I: Ord> Ord for (A, B, C, D, E, F, G, H, I) {
    fn cmp(&self, other: &(A, B, C, D, E, F, G, H, I)) -> Ordering {
        match Ord::cmp(&self.0, &other.0) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.1, &other.1) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.2, &other.2) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.3, &other.3) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.4, &other.4) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.5, &other.5) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.6, &other.6) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.7, &other.7) {
            Equal => {}
            ordering => return ordering,
        }
        Ord::cmp(&self.8, &other.8)
    }
}
impl<A: PartialEq, B: PartialEq, C: PartialEq, D: PartialEq, E: PartialEq, F: PartialEq, G: PartialEq, H: PartialEq, I: PartialEq, J: PartialEq> PartialEq for (A, B, C, D, E, F, G, H, I, J) {
    fn eq(&self, other: &(A, B, C, D, E, F, G, H, I, J)) -> bool {
        self.0 == other.0 && self.1 == other.1 && self.2 == other.2 && self.3 == other.3 && self.4 == other.4 && self.5 == other.5 && self.6 == other.6 && self.7 == other.7 && self.8 == other.8 && self.9 == other.9
    }
    fn ne(&self, other: &(A, B, C, D, E, F, G, H, I, J)) -> bool {
        self.0 != other.0 || self.1 != other.1 || self.2 != other.2 || self.3 != other.3 || self.4 != other.4 || self.5 != other.5 || self.6 != other.6 || self.7 != other.7 || self.8 != other.8 || self.9 != other.9
    }
}
impl<A: Eq, B: Eq, C: Eq, D: Eq, E: Eq, F: Eq, G: Eq, H: Eq, I: Eq, J: Eq> Eq for (A, B, C, D, E, F, G, H, I, J) {}
impl<A: PartialOrd, B: PartialOrd, C: PartialOrd, D: PartialOrd, E: PartialOrd, F: PartialOrd, G: PartialOrd, H: PartialOrd, I: PartialOrd, J: PartialOrd> PartialOrd for (A, B, C, D, E, F, G, H, I, J) {
    fn partial_cmp(&self, other: &(A, B, C, D, E, F, G, H, I, J)) -> Option<Ordering> {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.7, &other.7) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.8, &other.8) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        PartialOrd::partial_cmp(&self.9, &other.9)
    }

    fn lt(&self, other: &(A, B, C, D, E, F, G, H, I, J)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.7, &other.7) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.8, &other.8) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.9 < other.9
    }

    fn le(&self, other: &(A, B, C, D, E, F, G, H, I, J)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.7, &other.7) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.8, &other.8) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.9 <= other.9
    }

    fn gt(&self, other: &(A, B, C, D, E, F, G, H, I, J)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.7, &other.7) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.8, &other.8) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.9 > other.9
    }

    fn ge(&self, other: &(A, B, C, D, E, F, G, H, I, J)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.7, &other.7) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.8, &other.8) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.9 >= other.9
    }
}
impl<A: Ord, B: Ord, C: Ord, D: Ord, E: Ord, F: Ord, G: Ord, H: Ord, I: Ord, J: Ord> Ord for (A, B, C, D, E, F, G, H, I, J) {
    fn cmp(&self, other: &(A, B, C, D, E, F, G, H, I, J)) -> Ordering {
        match Ord::cmp(&self.0, &other.0) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.1, &other.1) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.2, &other.2) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.3, &other.3) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.4, &other.4) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.5, &other.5) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.6, &other.6) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.7, &other.7) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.8, &other.8) {
            Equal => {}
            ordering => return ordering,
        }
        Ord::cmp(&self.9, &other.9)
    }
}
impl<A: PartialEq, B: PartialEq, C: PartialEq, D: PartialEq, E: PartialEq, F: PartialEq, G: PartialEq, H: PartialEq, I: PartialEq, J: PartialEq, K: PartialEq> PartialEq for (A, B, C, D, E, F, G, H, I, J, K) {
    fn eq(&self, other: &(A, B, C, D, E, F, G, H, I, J, K)) -> bool {
        self.0 == other.0 && self.1 == other.1 && self.2 == other.2 && self.3 == other.3 && self.4 == other.4 && self.5 == other.5 && self.6 == other.6 && self.7 == other.7 && self.8 == other.8 && self.9 == other.9 && self.10 == other.10
    }
    fn ne(&self, other: &(A, B, C, D, E, F, G, H, I, J, K)) -> bool {
        self.0 != other.0 || self.1 != other.1 || self.2 != other.2 || self.3 != other.3 || self.4 != other.4 || self.5 != other.5 || self.6 != other.6 || self.7 != other.7 || self.8 != other.8 || self.9 != other.9 || self.10 != other.10
    }
}
impl<A: Eq, B: Eq, C: Eq, D: Eq, E: Eq, F: Eq, G: Eq, H: Eq, I: Eq, J: Eq, K: Eq> Eq for (A, B, C, D, E, F, G, H, I, J, K) {}
impl<A: PartialOrd, B: PartialOrd, C: PartialOrd, D: PartialOrd, E: PartialOrd, F: PartialOrd, G: PartialOrd, H: PartialOrd, I: PartialOrd, J: PartialOrd, K: PartialOrd> PartialOrd for (A, B, C, D, E, F, G, H, I, J, K) {
    fn partial_cmp(&self, other: &(A, B, C, D, E, F, G, H, I, J, K)) -> Option<Ordering> {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.7, &other.7) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.8, &other.8) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.9, &other.9) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        PartialOrd::partial_cmp(&self.10, &other.10)
    }

    fn lt(&self, other: &(A, B, C, D, E, F, G, H, I, J, K)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.7, &other.7) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.8, &other.8) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.9, &other.9) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.10 < other.10
    }

    fn le(&self, other: &(A, B, C, D, E, F, G, H, I, J, K)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.7, &other.7) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.8, &other.8) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.9, &other.9) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.10 <= other.10
    }

    fn gt(&self, other: &(A, B, C, D, E, F, G, H, I, J, K)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.7, &other.7) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.8, &other.8) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.9, &other.9) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.10 > other.10
    }

    fn ge(&self, other: &(A, B, C, D, E, F, G, H, I, J, K)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.7, &other.7) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.8, &other.8) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.9, &other.9) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.10 >= other.10
    }
}
impl<A: Ord, B: Ord, C: Ord, D: Ord, E: Ord, F: Ord, G: Ord, H: Ord, I: Ord, J: Ord, K: Ord> Ord for (A, B, C, D, E, F, G, H, I, J, K) {
    fn cmp(&self, other: &(A, B, C, D, E, F, G, H, I, J, K)) -> Ordering {
        match Ord::cmp(&self.0, &other.0) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.1, &other.1) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.2, &other.2) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.3, &other.3) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.4, &other.4) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.5, &other.5) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.6, &other.6) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.7, &other.7) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.8, &other.8) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.9, &other.9) {
            Equal => {}
            ordering => return ordering,
        }
        Ord::cmp(&self.10, &other.10)
    }
}
impl<A: PartialEq, B: PartialEq, C: PartialEq, D: PartialEq, E: PartialEq, F: PartialEq, G: PartialEq, H: PartialEq, I: PartialEq, J: PartialEq, K: PartialEq, L: PartialEq> PartialEq for (A, B, C, D, E, F, G, H, I, J, K, L) {
    fn eq(&self, other: &(A, B, C, D, E, F, G, H, I, J, K, L)) -> bool {
        self.0 == other.0 && self.1 == other.1 && self.2 == other.2 && self.3 == other.3 && self.4 == other.4 && self.5 == other.5 && self.6 == other.6 && self.7 == other.7 && self.8 == other.8 && self.9 == other.9 && self.10 == other.10 && self.11 == other.11
    }
    fn ne(&self, other: &(A, B, C, D, E, F, G, H, I, J, K, L)) -> bool {
        self.0 != other.0 || self.1 != other.1 || self.2 != other.2 || self.3 != other.3 || self.4 != other.4 || self.5 != other.5 || self.6 != other.6 || self.7 != other.7 || self.8 != other.8 || self.9 != other.9 || self.10 != other.10 || self.11 != other.11
    }
}
impl<A: Eq, B: Eq, C: Eq, D: Eq, E: Eq, F: Eq, G: Eq, H: Eq, I: Eq, J: Eq, K: Eq, L: Eq> Eq for (A, B, C, D, E, F, G, H, I, J, K, L) {}
impl<A: PartialOrd, B: PartialOrd, C: PartialOrd, D: PartialOrd, E: PartialOrd, F: PartialOrd, G: PartialOrd, H: PartialOrd, I: PartialOrd, J: PartialOrd, K: PartialOrd, L: PartialOrd> PartialOrd for (A, B, C, D, E, F, G, H, I, J, K, L) {
    fn partial_cmp(&self, other: &(A, B, C, D, E, F, G, H, I, J, K, L)) -> Option<Ordering> {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.7, &other.7) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.8, &other.8) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.9, &other.9) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        match PartialOrd::partial_cmp(&self.10, &other.10) {
            Some(Equal) => {}
            ordering => return ordering,
        }
        PartialOrd::partial_cmp(&self.11, &other.11)
    }

    fn lt(&self, other: &(A, B, C, D, E, F, G, H, I, J, K, L)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.7, &other.7) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.8, &other.8) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.9, &other.9) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.10, &other.10) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.11 < other.11
    }

    fn le(&self, other: &(A, B, C, D, E, F, G, H, I, J, K, L)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.7, &other.7) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.8, &other.8) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.9, &other.9) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.10, &other.10) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Less,
            None => return false,
        }
        self.11 <= other.11
    }

    fn gt(&self, other: &(A, B, C, D, E, F, G, H, I, J, K, L)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.7, &other.7) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.8, &other.8) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.9, &other.9) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.10, &other.10) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.11 > other.11
    }

    fn ge(&self, other: &(A, B, C, D, E, F, G, H, I, J, K, L)) -> bool {
        match PartialOrd::partial_cmp(&self.0, &other.0) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.1, &other.1) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.2, &other.2) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.3, &other.3) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.4, &other.4) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.5, &other.5) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.6, &other.6) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.7, &other.7) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.8, &other.8) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.9, &other.9) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        match PartialOrd::partial_cmp(&self.10, &other.10) {
            Some(Equal) => {}
            Some(ordering) => return ordering == Greater,
            None => return false,
        }
        self.11 >= other.11
    }
}
impl<A: Ord, B: Ord, C: Ord, D: Ord, E: Ord, F: Ord, G: Ord, H: Ord, I: Ord, J: Ord, K: Ord, L: Ord> Ord for (A, B, C, D, E, F, G, H, I, J, K, L) {
    fn cmp(&self, other: &(A, B, C, D, E, F, G, H, I, J, K, L)) -> Ordering {
        match Ord::cmp(&self.0, &other.0) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.1, &other.1) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.2, &other.2) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.3, &other.3) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.4, &other.4) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.5, &other.5) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.6, &other.6) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.7, &other.7) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.8, &other.8) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.9, &other.9) {
            Equal => {}
            ordering => return ordering,
        }
        match Ord::cmp(&self.10, &other.10) {
            Equal => {}
            ordering => return ordering,
        }
        Ord::cmp(&self.11, &other.11)
    }
}
