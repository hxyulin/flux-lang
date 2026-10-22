//! Explicit duplication of values: `x.clone()`.

/// Types whose values can be duplicated, perhaps at a cost:
/// `x.clone()` is a new value equal to `x`. `#[derive(Clone)]` on a
/// struct clones each of its fields.
#[lang = "clone"]
pub trait Clone: Sized {
    /// A duplicate of `self`.
    fn clone(&self) -> Self;

    /// Makes `self` a duplicate of `source`.
    fn clone_from(&mut self, source: &Self) {
        *self = source.clone()
    }
}

/// What `#[derive(Clone, Copy)]` calls for each field, whose `clone`
/// copies the struct: each field must still be `Clone`.
#[doc(hidden)]
pub fn assert_param_is_clone<T: Clone>(_param: &T) {}

// The primitive types and raw pointers are `Copy`: cloning them copies
// them. Shared references are too, built into the compiler until
// functions may return references.

impl Clone for i8 { fn clone(&self) -> i8 { *self } }
impl Clone for i16 { fn clone(&self) -> i16 { *self } }
impl Clone for i32 { fn clone(&self) -> i32 { *self } }
impl Clone for i64 { fn clone(&self) -> i64 { *self } }
impl Clone for i128 { fn clone(&self) -> i128 { *self } }
impl Clone for isize { fn clone(&self) -> isize { *self } }
impl Clone for u8 { fn clone(&self) -> u8 { *self } }
impl Clone for u16 { fn clone(&self) -> u16 { *self } }
impl Clone for u32 { fn clone(&self) -> u32 { *self } }
impl Clone for u64 { fn clone(&self) -> u64 { *self } }
impl Clone for u128 { fn clone(&self) -> u128 { *self } }
impl Clone for usize { fn clone(&self) -> usize { *self } }
impl Clone for bool { fn clone(&self) -> bool { *self } }
impl Clone for () { fn clone(&self) -> () { () } }
impl<T> Clone for *const T { fn clone(&self) -> Self { *self } }
impl<T> Clone for *mut T { fn clone(&self) -> Self { *self } }
// Function pointers, which rustc's `core` clones through a built-in
// impl for every arity.
impl<R> Clone for fn() -> R { fn clone(&self) -> Self { *self } }
impl<A, R> Clone for fn(A) -> R { fn clone(&self) -> Self { *self } }
impl<A, B, R> Clone for fn(A, B) -> R { fn clone(&self) -> Self { *self } }
impl<A, B, C, R> Clone for fn(A, B, C) -> R { fn clone(&self) -> Self { *self } }

// Tuples clone each element.
impl<A: Clone> Clone for (A,) {
    fn clone(&self) -> Self {
        (self.0.clone(),)
    }
}
impl<A: Clone, B: Clone> Clone for (A, B) {
    fn clone(&self) -> Self {
        (self.0.clone(), self.1.clone())
    }
}
impl<A: Clone, B: Clone, C: Clone> Clone for (A, B, C) {
    fn clone(&self) -> Self {
        (self.0.clone(), self.1.clone(), self.2.clone())
    }
}
impl<A: Clone, B: Clone, C: Clone, D: Clone> Clone for (A, B, C, D) {
    fn clone(&self) -> Self {
        (self.0.clone(), self.1.clone(), self.2.clone(), self.3.clone())
    }
}
impl<A: Clone, B: Clone, C: Clone, D: Clone, E: Clone> Clone for (A, B, C, D, E) {
    fn clone(&self) -> Self {
        (self.0.clone(), self.1.clone(), self.2.clone(), self.3.clone(), self.4.clone())
    }
}
impl<A: Clone, B: Clone, C: Clone, D: Clone, E: Clone, F: Clone> Clone for (A, B, C, D, E, F) {
    fn clone(&self) -> Self {
        (self.0.clone(), self.1.clone(), self.2.clone(), self.3.clone(), self.4.clone(), self.5.clone())
    }
}
impl<A: Clone, B: Clone, C: Clone, D: Clone, E: Clone, F: Clone, G: Clone> Clone for (A, B, C, D, E, F, G) {
    fn clone(&self) -> Self {
        (self.0.clone(), self.1.clone(), self.2.clone(), self.3.clone(), self.4.clone(), self.5.clone(), self.6.clone())
    }
}
impl<A: Clone, B: Clone, C: Clone, D: Clone, E: Clone, F: Clone, G: Clone, H: Clone> Clone for (A, B, C, D, E, F, G, H) {
    fn clone(&self) -> Self {
        (self.0.clone(), self.1.clone(), self.2.clone(), self.3.clone(), self.4.clone(), self.5.clone(), self.6.clone(), self.7.clone())
    }
}
impl<A: Clone, B: Clone, C: Clone, D: Clone, E: Clone, F: Clone, G: Clone, H: Clone, I: Clone> Clone for (A, B, C, D, E, F, G, H, I) {
    fn clone(&self) -> Self {
        (self.0.clone(), self.1.clone(), self.2.clone(), self.3.clone(), self.4.clone(), self.5.clone(), self.6.clone(), self.7.clone(), self.8.clone())
    }
}
impl<A: Clone, B: Clone, C: Clone, D: Clone, E: Clone, F: Clone, G: Clone, H: Clone, I: Clone, J: Clone> Clone for (A, B, C, D, E, F, G, H, I, J) {
    fn clone(&self) -> Self {
        (self.0.clone(), self.1.clone(), self.2.clone(), self.3.clone(), self.4.clone(), self.5.clone(), self.6.clone(), self.7.clone(), self.8.clone(), self.9.clone())
    }
}
impl<A: Clone, B: Clone, C: Clone, D: Clone, E: Clone, F: Clone, G: Clone, H: Clone, I: Clone, J: Clone, K: Clone> Clone for (A, B, C, D, E, F, G, H, I, J, K) {
    fn clone(&self) -> Self {
        (self.0.clone(), self.1.clone(), self.2.clone(), self.3.clone(), self.4.clone(), self.5.clone(), self.6.clone(), self.7.clone(), self.8.clone(), self.9.clone(), self.10.clone())
    }
}
impl<A: Clone, B: Clone, C: Clone, D: Clone, E: Clone, F: Clone, G: Clone, H: Clone, I: Clone, J: Clone, K: Clone, L: Clone> Clone for (A, B, C, D, E, F, G, H, I, J, K, L) {
    fn clone(&self) -> Self {
        (self.0.clone(), self.1.clone(), self.2.clone(), self.3.clone(), self.4.clone(), self.5.clone(), self.6.clone(), self.7.clone(), self.8.clone(), self.9.clone(), self.10.clone(), self.11.clone())
    }
}
