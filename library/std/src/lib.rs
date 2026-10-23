//! A bounded std facade over Flux's implemented core and alloc APIs.
//! Reexports preserve the original definitions and trait implementations.

extern crate alloc;

pub use ::alloc::{alloc, borrow, boxed, collections, fmt, format, prelude, string, vec};
pub use core::{
    any, array, ascii, cell, char, clone, cmp, convert, default, ffi, hash, hint, iter, marker,
    mem, num, ops, option, panic, ptr, result,
};
pub use core::{dbg, eprint, eprintln, print, println};
pub use core::{slice, str, u8, u32};

pub mod thread;

pub mod sync {
    pub use ::alloc::sync::{Arc, Weak};
    pub use core::sync::atomic;
}
