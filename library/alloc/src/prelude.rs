//! The names the program sees without importing them: `core`'s, and
//! `Box`, `Vec`, `String`, `ToString` and `ToOwned`, as rustc's `std`
//! prelude has them.

pub mod rust_2024 {
    pub use core::prelude::rust_2024::*;

    pub use crate::borrow::ToOwned;
    pub use crate::boxed::Box;
    pub use crate::string::{String, ToString};
    pub use crate::vec::Vec;
}

pub mod rust_2018 {
    pub use core::prelude::rust_2018::*;
    pub use crate::borrow::ToOwned;
    pub use crate::boxed::Box;
    pub use crate::string::{String, ToString};
    pub use crate::vec::Vec;
}

/// The edition-independent prelude, which the 2015 and 2018 editions use.
pub mod v1 {
    pub use super::rust_2018::*;
}
pub mod rust_2015 {
    pub use super::rust_2018::*;
}
pub mod rust_2021 {
    pub use super::rust_2024::*;
}
