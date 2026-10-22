//! flux's core library: the parts of Rust's `core` that flux supports,
//! written in flux and built into the compiler.
//!
//! Every program can name it as `core`, and gets `prelude::rust_2024`
//! imported, as rustc does with its own.

#[macro_use]
mod macros;

pub mod alloc;
pub mod array;
pub mod ascii;
pub mod cell;
mod bool;
pub mod borrow;
pub mod char;
pub mod clone;
pub mod cmp;
pub mod convert;
pub mod default;
pub mod ffi;
pub mod fmt;
pub mod hash;
pub mod hint;
mod intrinsics;
#[doc(hidden)]
pub mod io;
pub mod iter;
pub mod marker;
pub mod mem;
pub mod num;
pub mod ops;
pub mod option;
pub mod panic;
#[doc(hidden)]
pub mod panicking;
pub mod prelude;
pub mod ptr;
pub mod result;
pub mod slice;
pub mod str;
mod unicode;

/// Legacy integer constants used by existing Rust libraries.
pub mod u8 {
    pub const MIN: u8 = 0;
    pub const MAX: u8 = 255;
}

/// Legacy integer constants used by existing Rust libraries.
pub mod u32 {
    pub const MIN: u32 = 0;
    pub const MAX: u32 = 4_294_967_295;
}
