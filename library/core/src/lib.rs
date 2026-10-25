//! flux's core library: the parts of Rust's `core` that flux supports,
//! written in flux and built into the compiler.
//!
//! Every program can name it as `core`, and gets `prelude::rust_2024`
//! imported, as rustc does with its own.

#[macro_use]
mod macros;

pub mod alloc;
pub mod any;
pub mod array;
#[cfg(target_arch = "aarch64")]
pub mod arch;
pub mod ascii;
pub mod cell;
mod bool;
pub mod borrow;
pub mod char;
mod char_properties;
pub mod clone;
pub mod cmp;
pub mod convert;
pub mod default;
pub mod error;
pub mod ffi;
pub mod fmt;
mod float;
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
pub mod sync;
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

/// Legacy pointer-sized unsigned integer constants.
pub mod usize {
    pub const MIN: usize = 0;
    pub const MAX: usize = !0usize;
}

/// Legacy pointer-sized signed integer constants.
pub mod isize {
    pub const MAX: isize = (!0usize >> 1) as isize;
    pub const MIN: isize = -MAX - 1;
}
