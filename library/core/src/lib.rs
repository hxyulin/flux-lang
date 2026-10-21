//! flux's core library: the parts of Rust's `core` that flux supports,
//! written in flux and built into the compiler.
//!
//! Every program can name it as `core`, and gets `prelude::rust_2024`
//! imported, as rustc does with its own.

pub mod cmp;
pub mod convert;
pub mod marker;
pub mod mem;
pub mod num;
pub mod ops;
pub mod prelude;
