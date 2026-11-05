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

/// Legacy integer constants used by existing Rust libraries.
pub mod i8 {
    pub const MIN: i8 = <i8>::MIN;
    pub const MAX: i8 = <i8>::MAX;
}

/// Legacy integer constants used by existing Rust libraries.
pub mod i16 {
    pub const MIN: i16 = <i16>::MIN;
    pub const MAX: i16 = <i16>::MAX;
}

/// Legacy integer constants used by existing Rust libraries.
pub mod i32 {
    pub const MIN: i32 = <i32>::MIN;
    pub const MAX: i32 = <i32>::MAX;
}

/// Legacy integer constants used by existing Rust libraries.
pub mod i64 {
    pub const MIN: i64 = <i64>::MIN;
    pub const MAX: i64 = <i64>::MAX;
}

/// Legacy integer constants used by existing Rust libraries.
pub mod i128 {
    pub const MIN: i128 = <i128>::MIN;
    pub const MAX: i128 = <i128>::MAX;
}

/// Legacy integer constants used by existing Rust libraries.
pub mod u16 {
    pub const MIN: u16 = <u16>::MIN;
    pub const MAX: u16 = <u16>::MAX;
}

/// Legacy integer constants used by existing Rust libraries.
pub mod u64 {
    pub const MIN: u64 = <u64>::MIN;
    pub const MAX: u64 = <u64>::MAX;
}

/// Legacy integer constants used by existing Rust libraries.
pub mod u128 {
    pub const MIN: u128 = <u128>::MIN;
    pub const MAX: u128 = <u128>::MAX;
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
    pub const RADIX: u32 = <f32>::RADIX;
    pub const MANTISSA_DIGITS: u32 = <f32>::MANTISSA_DIGITS;
    pub const DIGITS: u32 = <f32>::DIGITS;
    pub const MIN_EXP: i32 = <f32>::MIN_EXP;
    pub const MAX_EXP: i32 = <f32>::MAX_EXP;
    pub const MIN_10_EXP: i32 = <f32>::MIN_10_EXP;
    pub const MAX_10_EXP: i32 = <f32>::MAX_10_EXP;

    /// Mathematical constants, rounded to f32.
    pub mod consts {
        pub const PI: f32 = 3.14159265358979323846264338327950288_f32;
        pub const TAU: f32 = 6.28318530717958647692528676655900577_f32;
        pub const FRAC_PI_2: f32 = 1.57079632679489661923132169163975144_f32;
        pub const FRAC_PI_3: f32 = 1.04719755119659774615421446109316763_f32;
        pub const FRAC_PI_4: f32 = 0.785398163397448309615660845819875721_f32;
        pub const FRAC_PI_6: f32 = 0.52359877559829887307710723054658381_f32;
        pub const FRAC_PI_8: f32 = 0.39269908169872415480783042290993786_f32;
        pub const FRAC_1_PI: f32 = 0.318309886183790671537767526745028724_f32;
        pub const FRAC_2_PI: f32 = 0.636619772367581343075535053490057448_f32;
        pub const FRAC_2_SQRT_PI: f32 = 1.12837916709551257389615890312154517_f32;
        pub const SQRT_2: f32 = 1.41421356237309504880168872420969808_f32;
        pub const FRAC_1_SQRT_2: f32 = 0.707106781186547524400844362104849039_f32;
        pub const E: f32 = 2.71828182845904523536028747135266250_f32;
        pub const LOG2_E: f32 = 1.44269504088896340735992468100189214_f32;
        pub const LOG2_10: f32 = 3.32192809488736234787031942948939018_f32;
        pub const LOG10_E: f32 = 0.434294481903251827651128918916605082_f32;
        pub const LOG10_2: f32 = 0.301029995663981195213738894724493027_f32;
        pub const LN_2: f32 = 0.693147180559945309417232121458176568_f32;
        pub const LN_10: f32 = 2.30258509299404568401799145468436421_f32;
    }
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
    pub const RADIX: u32 = <f64>::RADIX;
    pub const MANTISSA_DIGITS: u32 = <f64>::MANTISSA_DIGITS;
    pub const DIGITS: u32 = <f64>::DIGITS;
    pub const MIN_EXP: i32 = <f64>::MIN_EXP;
    pub const MAX_EXP: i32 = <f64>::MAX_EXP;
    pub const MIN_10_EXP: i32 = <f64>::MIN_10_EXP;
    pub const MAX_10_EXP: i32 = <f64>::MAX_10_EXP;

    /// Mathematical constants, rounded to f64.
    pub mod consts {
        pub const PI: f64 = 3.14159265358979323846264338327950288_f64;
        pub const TAU: f64 = 6.28318530717958647692528676655900577_f64;
        pub const FRAC_PI_2: f64 = 1.57079632679489661923132169163975144_f64;
        pub const FRAC_PI_3: f64 = 1.04719755119659774615421446109316763_f64;
        pub const FRAC_PI_4: f64 = 0.785398163397448309615660845819875721_f64;
        pub const FRAC_PI_6: f64 = 0.52359877559829887307710723054658381_f64;
        pub const FRAC_PI_8: f64 = 0.39269908169872415480783042290993786_f64;
        pub const FRAC_1_PI: f64 = 0.318309886183790671537767526745028724_f64;
        pub const FRAC_2_PI: f64 = 0.636619772367581343075535053490057448_f64;
        pub const FRAC_2_SQRT_PI: f64 = 1.12837916709551257389615890312154517_f64;
        pub const SQRT_2: f64 = 1.41421356237309504880168872420969808_f64;
        pub const FRAC_1_SQRT_2: f64 = 0.707106781186547524400844362104849039_f64;
        pub const E: f64 = 2.71828182845904523536028747135266250_f64;
        pub const LOG2_10: f64 = 3.32192809488736234787031942948939018_f64;
        pub const LOG2_E: f64 = 1.44269504088896340735992468100189214_f64;
        pub const LOG10_2: f64 = 0.301029995663981195213738894724493027_f64;
        pub const LOG10_E: f64 = 0.434294481903251827651128918916605082_f64;
        pub const LN_2: f64 = 0.693147180559945309417232121458176568_f64;
        pub const LN_10: f64 = 2.30258509299404568401799145468436421_f64;
    }
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
