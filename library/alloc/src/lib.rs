//! flux's alloc library: memory from the heap, through C's `malloc`,
//! `realloc` and `free`, and the types that own it: `Box`, `Vec` and
//! `String`. Like `core`, it is written in flux and built into the
//! compiler; the program's prelude is its `prelude::rust_2024`, which
//! has `core`'s names and these, as rustc's `std` prelude does, and
//! `extern crate alloc;` lets the program name it by path.

#[macro_use]
mod macros;

pub mod alloc;
pub mod borrow;
pub mod boxed;
pub mod collections;
pub mod fmt;
pub mod prelude;
mod raw_vec;
pub mod slice;
pub mod str;
pub mod string;
pub mod sync;
pub mod vec;
