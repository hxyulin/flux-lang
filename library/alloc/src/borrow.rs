//! `ToOwned`: owned copies of borrowed data, `"a".to_owned()` a `String`
//! and `[1, 2][..].to_owned()` a `Vec`.

pub use core::borrow::{Borrow, BorrowMut};

use crate::string::String;
use crate::vec::Vec;

/// A type that has an owned form, made from a reference to it.
pub trait ToOwned {
    type Owned: Borrow<Self>;

    fn to_owned(&self) -> Self::Owned;
}

impl<T: Clone> ToOwned for T {
    type Owned = T;

    fn to_owned(&self) -> T {
        self.clone()
    }
}

impl ToOwned for str {
    type Owned = String;

    fn to_owned(&self) -> String {
        String::from(self)
    }
}

impl<T: Clone> ToOwned for [T] {
    type Owned = Vec<T>;

    fn to_owned(&self) -> Vec<T> {
        self.to_vec()
    }
}

/// Borrowed data or its owned form. Mutation copies borrowed data on demand.
pub enum Cow<'a, B: ToOwned + ?Sized> {
    Borrowed(&'a B),
    Owned(B::Owned),
}

impl<'a, B: ToOwned + ?Sized> Cow<'a, B> {
    pub fn into_owned(self) -> B::Owned {
        match self {
            Cow::Borrowed(value) => value.to_owned(),
            Cow::Owned(value) => value,
        }
    }

    pub fn to_mut(&mut self) -> &mut B::Owned {
        if let Cow::Borrowed(value) = self {
            *self = Cow::Owned((*value).to_owned());
        }
        match self {
            Cow::Owned(value) => value,
            Cow::Borrowed(_) => unreachable!(),
        }
    }
}

impl<B: ToOwned + ?Sized> core::ops::Deref for Cow<'_, B> {
    type Target = B;

    fn deref(&self) -> &B {
        match self {
            Cow::Borrowed(value) => value,
            Cow::Owned(value) => value.borrow(),
        }
    }
}

impl<B: ToOwned + ?Sized> core::convert::AsRef<B> for Cow<'_, B> {
    fn as_ref(&self) -> &B {
        self
    }
}

impl<B: ToOwned + ?Sized> Borrow<B> for Cow<'_, B> {
    fn borrow(&self) -> &B {
        self
    }
}

impl<B: ToOwned + ?Sized> Clone for Cow<'_, B> {
    fn clone(&self) -> Self {
        match self {
            Cow::Borrowed(value) => Cow::Borrowed(value),
            Cow::Owned(value) => {
                let borrowed: &B = value.borrow();
                Cow::Owned(borrowed.to_owned())
            }
        }
    }
}

impl<B: ToOwned + core::fmt::Debug + ?Sized> core::fmt::Debug for Cow<'_, B> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&**self, f)
    }
}

impl<B: ToOwned + core::fmt::Display + ?Sized> core::fmt::Display for Cow<'_, B> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(&**self, f)
    }
}

impl<B: ToOwned + ?Sized, C: ToOwned + ?Sized> PartialEq<Cow<'_, C>> for Cow<'_, B>
where B: PartialEq<C> {
    fn eq(&self, other: &Cow<'_, C>) -> bool { **self == **other }
}
impl<B: ToOwned + Eq + ?Sized> Eq for Cow<'_, B> {}

impl<T: Clone + PartialEq<U>, U> PartialEq<Vec<U>> for Cow<'_, [T]> {
    fn eq(&self, other: &Vec<U>) -> bool { **self == *other.as_slice() }
}
impl<T: Clone + PartialEq<U>, U> PartialEq<&[U]> for Cow<'_, [T]> {
    fn eq(&self, other: &&[U]) -> bool { **self == **other }
}
impl<T: Clone + PartialEq<U>, U> PartialEq<&mut [U]> for Cow<'_, [T]> {
    fn eq(&self, other: &&mut [U]) -> bool { **self == **other }
}

impl PartialEq<str> for Cow<'_, str> {
    fn eq(&self, other: &str) -> bool { **self == *other }
}
impl PartialEq<&str> for Cow<'_, str> {
    fn eq(&self, other: &&str) -> bool { **self == **other }
}
impl PartialEq<Cow<'_, str>> for str {
    fn eq(&self, other: &Cow<'_, str>) -> bool { *self == **other }
}
impl PartialEq<Cow<'_, str>> for &str {
    fn eq(&self, other: &Cow<'_, str>) -> bool { **self == **other }
}

impl<'a, T: Clone> From<Vec<T>> for Cow<'a, [T]> {
    fn from(value: Vec<T>) -> Self { Cow::Owned(value) }
}

impl<'a> From<&'a str> for Cow<'a, str> {
    fn from(value: &'a str) -> Self { Cow::Borrowed(value) }
}
impl<'a> From<&'a String> for Cow<'a, str> {
    fn from(value: &'a String) -> Self { Cow::Borrowed(value.as_str()) }
}
impl From<String> for Cow<'_, str> {
    fn from(value: String) -> Self { Cow::Owned(value) }
}

impl<'a, T: Clone> From<&'a [T]> for Cow<'a, [T]> {
    fn from(value: &'a [T]) -> Self { Cow::Borrowed(value) }
}
impl<'a, T: Clone, const N: usize> From<&'a [T; N]> for Cow<'a, [T]> {
    fn from(value: &'a [T; N]) -> Self { Cow::Borrowed(value as &[T]) }
}
impl<'a, T: Clone> From<&'a Vec<T>> for Cow<'a, [T]> {
    fn from(value: &'a Vec<T>) -> Self { Cow::Borrowed(value.as_slice()) }
}
impl<'a> From<Cow<'a, str>> for String {
    fn from(value: Cow<'a, str>) -> String { value.into_owned() }
}
impl<'a, T: Clone> From<Cow<'a, [T]>> for Vec<T> {
    fn from(value: Cow<'a, [T]>) -> Vec<T> { value.into_owned() }
}
impl<'a> From<Cow<'a, str>> for crate::boxed::Box<str> {
    fn from(value: Cow<'a, str>) -> crate::boxed::Box<str> { value.into_owned().into_boxed_str() }
}
impl<'a, T: Clone> From<Cow<'a, [T]>> for crate::boxed::Box<[T]> {
    fn from(value: Cow<'a, [T]>) -> crate::boxed::Box<[T]> { value.into_owned().into_boxed_slice() }
}
impl<'a, T: Clone> FromIterator<T> for Cow<'a, [T]> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Cow<'a, [T]> { Cow::Owned(iter.into_iter().collect()) }
}
impl<'a> FromIterator<char> for Cow<'a, str> {
    fn from_iter<I: IntoIterator<Item = char>>(iter: I) -> Cow<'a, str> { Cow::Owned(iter.into_iter().collect()) }
}
impl<'a, 'b> FromIterator<&'b str> for Cow<'a, str> {
    fn from_iter<I: IntoIterator<Item = &'b str>>(iter: I) -> Cow<'a, str> { Cow::Owned(iter.into_iter().collect()) }
}
impl<'a> FromIterator<String> for Cow<'a, str> {
    fn from_iter<I: IntoIterator<Item = String>>(iter: I) -> Cow<'a, str> { Cow::Owned(iter.into_iter().collect()) }
}
impl<B: ToOwned + ?Sized> Default for Cow<'_, B> where B::Owned: Default {
    fn default() -> Self { Cow::Owned(<B::Owned as Default>::default()) }
}
impl<'a, B: ToOwned + PartialOrd + ?Sized> PartialOrd for Cow<'a, B> {
    fn partial_cmp(&self, other: &Cow<'a, B>) -> Option<core::cmp::Ordering> { PartialOrd::partial_cmp(&**self, &**other) }
}
impl<B: ToOwned + Ord + ?Sized> Ord for Cow<'_, B> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering { Ord::cmp(&**self, &**other) }
}
impl<B: ToOwned + core::hash::Hash + ?Sized> core::hash::Hash for Cow<'_, B> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) { core::hash::Hash::hash(&**self, state) }
}
impl PartialEq<String> for Cow<'_, str> {
    fn eq(&self, other: &String) -> bool { **self == **other }
}
impl PartialEq<Cow<'_, str>> for String {
    fn eq(&self, other: &Cow<'_, str>) -> bool { **self == **other }
}
impl<'a> core::ops::Add<&'a str> for Cow<'a, str> {
    type Output = Cow<'a, str>;
    fn add(mut self, rhs: &'a str) -> Cow<'a, str> { self += rhs; self }
}
impl<'a> core::ops::AddAssign<&'a str> for Cow<'a, str> {
    fn add_assign(&mut self, rhs: &'a str) {
        if self.is_empty() {
            *self = Cow::Borrowed(rhs)
        } else if !rhs.is_empty() {
            if let Cow::Borrowed(lhs) = *self {
                let mut s = String::with_capacity(lhs.len() + rhs.len());
                s.push_str(lhs);
                *self = Cow::Owned(s);
            }
            self.to_mut().push_str(rhs);
        }
    }
}
