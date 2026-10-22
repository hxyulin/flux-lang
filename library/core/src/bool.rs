//! `bool`'s methods.

use crate::ops::FnOnce;
use crate::option::Option::{self, None, Some};

impl bool {
    /// `Some(t)` if it is `true`, else `None`; `t` is evaluated either
    /// way.
    pub fn then_some<T>(self, t: T) -> Option<T> {
        if self { Some(t) } else { None }
    }

    /// `Some(f())` if it is `true`, else `None`, without calling `f`.
    pub fn then<T, F: FnOnce() -> T>(self, f: F) -> Option<T> {
        if self { Some(f()) } else { None }
    }
}
