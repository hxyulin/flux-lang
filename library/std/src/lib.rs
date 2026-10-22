//! A bounded std facade over Flux's implemented core and alloc APIs.
//! Reexports preserve the original definitions and trait implementations.

extern crate alloc;

pub use ::alloc::{alloc, borrow, boxed, fmt, prelude, string, vec};
pub use core::{
    array, ascii, cell, char, clone, cmp, convert, default, ffi, hash, hint, iter, marker, mem,
    num, ops, option, panic, ptr, result,
};
pub use core::{slice, str};

pub mod thread;
