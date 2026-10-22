//! The names every module sees without importing them.

pub mod rust_2024 {
    pub use crate::clone::Clone;
    pub use crate::cmp::{Eq, Ord, PartialEq, PartialOrd};
    pub use crate::convert::{AsMut, AsRef, From, Into, TryFrom, TryInto};
    pub use crate::default::Default;
    pub use crate::iter::{
        DoubleEndedIterator, ExactSizeIterator, Extend, FromIterator, IntoIterator, Iterator,
    };
    pub use crate::marker::{Copy, Sized};
    pub use crate::mem::{align_of, align_of_val, drop, size_of, size_of_val};
    pub use crate::ops::{Drop, Fn, FnMut, FnOnce};
    pub use crate::option::Option;
    pub use crate::option::Option::{None, Some};
    pub use crate::result::Result;
    pub use crate::result::Result::{Err, Ok};
}
