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
