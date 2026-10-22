//! Inner doc comments stay with the file.
#![allow(dead_code)]

/// Doc comments attach to the item below them.
pub fn public() {}
pub(crate) const fn constant() -> u32 { 1 }
unsafe extern "C" fn callback(x: i32) -> i32 { x }
fn declaration(self_: u8);

struct Unit;
struct Tuple(pub u8, (u16, u32));
#[derive(Clone, Copy)]
pub struct Record<T> where T: Copy {
    pub(super) x: T,
    y: [u8; 4],
}

enum Shape {
    Circle { radius: f64 },
    Square(f64),
    Empty = 3,
}

// Visibility on a variant is an error after parsing, as in rustc.
enum Visible {
    pub A,
}

// So is an or-pattern parameter without parentheses.
fn alternatives(Some(x) | None: Option<i32>) {}

impl Shape {
    pub fn area(&self) -> f64 { 0.0 }
    fn consume(mut self) {}
    fn boxed(self: Box<Self>) {}
    fn named<'a>(&'a mut self) {}
}
impl<T: Copy> Trait<T> for Record<T> {
    type Out = T;
    const N: usize = 4;
}
pub trait Trait<T>: Sized where T: Copy {
    type Out;
    const N: usize;
    fn required(&self, x: T) -> Self::Out;
    fn provided() {}
}
unsafe impl Send for Unit {}

type Alias<T> = Record<T>;
const _: () = ();
static mut COUNTER: u32 = 0;

mod inline {
    use super::*;
}
mod outline;

use std::collections::{HashMap, hash_map::Entry as E};
use self::inline::*;
use ::core::mem;
pub use crate::{Shape as _, Unit};

extern crate alloc;
unsafe extern "C" {
    fn puts(s: *const u8) -> i32;
    static errno: i32;
}

macro_rules! square {
    ($x:expr) => { $x * $x };
}
square!(2);
thread_local! { static X: u8 = 0; }
unsafe extern "C" {
    fn printf(format: *const u8, ...) -> i32;
    fn open(path: *const u8, flags: i32, mode: ...) -> i32;
}
fn attributed(#[allow(unused)] x: u8, #[cfg(all())] y: u8) {}
