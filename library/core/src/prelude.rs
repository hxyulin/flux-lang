//! The names every module sees without importing them.

pub mod rust_2024 {
    pub use crate::clone::Clone;
    pub use crate::cmp::{Eq, PartialEq};
    pub use crate::marker::{Copy, Sized};
    pub use crate::mem::drop;
}
