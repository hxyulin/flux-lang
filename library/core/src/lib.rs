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
#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
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
pub mod future;
pub mod pin;
pub mod task;
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
#[cfg(target_os = "linux")]
#[doc(hidden)]
pub mod runtime;
pub mod slice;
pub mod str;
pub mod sync;
pub mod time;
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

/// Legacy floating-point constants.
pub mod f32 {
    pub const NAN: f32 = <f32>::NAN;
    pub const INFINITY: f32 = <f32>::INFINITY;
    pub const NEG_INFINITY: f32 = <f32>::NEG_INFINITY;
    pub const MIN: f32 = <f32>::MIN;
    pub const MAX: f32 = <f32>::MAX;
    pub const MIN_POSITIVE: f32 = <f32>::MIN_POSITIVE;
    pub const EPSILON: f32 = <f32>::EPSILON;
}

/// Legacy floating-point constants.
pub mod f64 {
    pub const NAN: f64 = <f64>::NAN;
    pub const INFINITY: f64 = <f64>::INFINITY;
    pub const NEG_INFINITY: f64 = <f64>::NEG_INFINITY;
    pub const MIN: f64 = <f64>::MIN;
    pub const MAX: f64 = <f64>::MAX;
    pub const MIN_POSITIVE: f64 = <f64>::MIN_POSITIVE;
    pub const EPSILON: f64 = <f64>::EPSILON;
}

// Separate aliases avoid self-referential names while exposing ordinary
// primitive paths and reexports through the existing type namespace.
mod primitive_types {
    pub type Bool = bool;
    pub type Char = char;
    pub type Str = str;
    pub type I8 = i8;
    pub type I16 = i16;
    pub type I32 = i32;
    pub type I64 = i64;
    pub type I128 = i128;
    pub type Isize = isize;
    pub type U8 = u8;
    pub type U16 = u16;
    pub type U32 = u32;
    pub type U64 = u64;
    pub type U128 = u128;
    pub type Usize = usize;
    pub type F32 = f32;
    pub type F64 = f64;
}
/// Primitive Rust types, independent of names shadowed in caller scopes.
pub mod primitive {
    pub use super::primitive_types::{
        Bool as bool,
        Char as char,
        Str as str,
        I8 as i8,
        I16 as i16,
        I32 as i32,
        I64 as i64,
        I128 as i128,
        Isize as isize,
        U8 as u8,
        U16 as u16,
        U32 as u32,
        U64 as u64,
        U128 as u128,
        Usize as usize,
        F32 as f32,
        F64 as f64,
    };
}
