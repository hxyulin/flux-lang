//! Traits that say what kind of type a type is.

use crate::clone::Clone;

/// Types whose size is known at compile time: every type flux has. A
/// trait's `Self` is not assumed to be, so a provided method that takes
/// `self` by value needs `where Self: Sized`.
#[lang = "sized"]
pub trait Sized {}

/// Types whose values are copied rather than moved: using one leaves it
/// usable. A struct is, if it implements `Copy`, which needs each of its
/// fields to be; `#[derive(Clone, Copy)]` implements both.
#[lang = "copy"]
pub trait Copy: Clone {}

impl Copy for i8 {}
impl Copy for i16 {}
impl Copy for i32 {}
impl Copy for i64 {}
impl Copy for i128 {}
impl Copy for isize {}
impl Copy for u8 {}
impl Copy for u16 {}
impl Copy for u32 {}
impl Copy for u64 {}
impl Copy for u128 {}
impl Copy for usize {}
impl Copy for bool {}
impl Copy for char {}
impl Copy for () {}
impl<T> Copy for *const T {}
impl<T> Copy for *mut T {}
impl<A: Copy> Copy for (A,) {}
impl<A: Copy, B: Copy> Copy for (A, B) {}
impl<A: Copy, B: Copy, C: Copy> Copy for (A, B, C) {}
impl<A: Copy, B: Copy, C: Copy, D: Copy> Copy for (A, B, C, D) {}
impl<A: Copy, B: Copy, C: Copy, D: Copy, E: Copy> Copy for (A, B, C, D, E) {}
impl<A: Copy, B: Copy, C: Copy, D: Copy, E: Copy, F: Copy> Copy for (A, B, C, D, E, F) {}
impl<A: Copy, B: Copy, C: Copy, D: Copy, E: Copy, F: Copy, G: Copy> Copy for (A, B, C, D, E, F, G) {}
impl<A: Copy, B: Copy, C: Copy, D: Copy, E: Copy, F: Copy, G: Copy, H: Copy> Copy for (A, B, C, D, E, F, G, H) {}
impl<A: Copy, B: Copy, C: Copy, D: Copy, E: Copy, F: Copy, G: Copy, H: Copy, I: Copy> Copy for (A, B, C, D, E, F, G, H, I) {}
impl<A: Copy, B: Copy, C: Copy, D: Copy, E: Copy, F: Copy, G: Copy, H: Copy, I: Copy, J: Copy> Copy for (A, B, C, D, E, F, G, H, I, J) {}
impl<A: Copy, B: Copy, C: Copy, D: Copy, E: Copy, F: Copy, G: Copy, H: Copy, I: Copy, J: Copy, K: Copy> Copy for (A, B, C, D, E, F, G, H, I, J, K) {}
impl<A: Copy, B: Copy, C: Copy, D: Copy, E: Copy, F: Copy, G: Copy, H: Copy, I: Copy, J: Copy, K: Copy, L: Copy> Copy for (A, B, C, D, E, F, G, H, I, J, K, L) {}
