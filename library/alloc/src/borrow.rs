//! `ToOwned`: owned copies of borrowed data, `"a".to_owned()` a `String`
//! and `[1, 2][..].to_owned()` a `Vec`.

pub use core::borrow::{Borrow, BorrowMut};

use crate::string::String;
use crate::vec::Vec;

/// A type that has an owned form, made from a reference to it.
pub trait ToOwned {
    type Owned;

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
