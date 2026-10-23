//! A bounded std facade over Flux's implemented core and alloc APIs.
//! Reexports preserve the original definitions and trait implementations.

extern crate alloc;

pub use ::alloc::{alloc, borrow, boxed, fmt, format, prelude, string, vec};
pub use core::{
    any, array, ascii, cell, char, clone, cmp, convert, default, ffi, hash, hint, iter, marker, mem,
    num, ops, option, panic, ptr, result,
};
pub use core::{slice, str, u8, u32};
pub use core::{dbg, eprint, eprintln, print, println};

pub mod thread;
