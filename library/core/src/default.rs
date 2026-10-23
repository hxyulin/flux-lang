//! Default values: `T::default()`.

/// Types with a default value. `#[derive(Default)]` on a struct makes
/// each field's default; on an enum, the variant marked `#[default]`.
/// Arrays of up to 32 elements whose elements have one have one too,
/// built into the compiler, as rustc's `core` makes them with macros.
#[lang = "Default"]
pub trait Default: Sized {
    /// The default value.
    fn default() -> Self;
}

// Written out for each type, since flux has no macros.
impl Default for i8 { fn default() -> i8 { 0 } }
impl Default for i16 { fn default() -> i16 { 0 } }
impl Default for i32 { fn default() -> i32 { 0 } }
impl Default for i64 { fn default() -> i64 { 0 } }
impl Default for i128 { fn default() -> i128 { 0 } }
impl Default for isize { fn default() -> isize { 0 } }
impl Default for u8 { fn default() -> u8 { 0 } }
impl Default for u16 { fn default() -> u16 { 0 } }
impl Default for u32 { fn default() -> u32 { 0 } }
impl Default for u64 { fn default() -> u64 { 0 } }
impl Default for u128 { fn default() -> u128 { 0 } }
impl Default for usize { fn default() -> usize { 0 } }
impl Default for bool { fn default() -> bool { false } }
impl Default for () { fn default() -> () { () } }
impl Default for char { fn default() -> char { '\x00' } }
impl Default for &str { fn default() -> Self { "" } }
impl<T> Default for &[T] { fn default() -> Self { &[] } }
impl<T> Default for &mut [T] { fn default() -> Self { &mut [] } }

impl<T> Default for crate::option::Option<T> {
    fn default() -> Self {
        crate::option::Option::None
    }
}

// Tuples of up to twelve elements, each its default.
impl<A: Default> Default for (A,) { fn default() -> Self { (A::default(),) } }
impl<A: Default, B: Default> Default for (A, B) { fn default() -> Self { (A::default(), B::default()) } }
impl<A: Default, B: Default, C: Default> Default for (A, B, C) { fn default() -> Self { (A::default(), B::default(), C::default()) } }
impl<A: Default, B: Default, C: Default, D: Default> Default for (A, B, C, D) { fn default() -> Self { (A::default(), B::default(), C::default(), D::default()) } }
impl<A: Default, B: Default, C: Default, D: Default, E: Default> Default for (A, B, C, D, E) { fn default() -> Self { (A::default(), B::default(), C::default(), D::default(), E::default()) } }
impl<A: Default, B: Default, C: Default, D: Default, E: Default, F: Default> Default for (A, B, C, D, E, F) { fn default() -> Self { (A::default(), B::default(), C::default(), D::default(), E::default(), F::default()) } }
impl<A: Default, B: Default, C: Default, D: Default, E: Default, F: Default, G: Default> Default for (A, B, C, D, E, F, G) { fn default() -> Self { (A::default(), B::default(), C::default(), D::default(), E::default(), F::default(), G::default()) } }
impl<A: Default, B: Default, C: Default, D: Default, E: Default, F: Default, G: Default, H: Default> Default for (A, B, C, D, E, F, G, H) { fn default() -> Self { (A::default(), B::default(), C::default(), D::default(), E::default(), F::default(), G::default(), H::default()) } }
impl<A: Default, B: Default, C: Default, D: Default, E: Default, F: Default, G: Default, H: Default, I: Default> Default for (A, B, C, D, E, F, G, H, I) { fn default() -> Self { (A::default(), B::default(), C::default(), D::default(), E::default(), F::default(), G::default(), H::default(), I::default()) } }
impl<A: Default, B: Default, C: Default, D: Default, E: Default, F: Default, G: Default, H: Default, I: Default, J: Default> Default for (A, B, C, D, E, F, G, H, I, J) { fn default() -> Self { (A::default(), B::default(), C::default(), D::default(), E::default(), F::default(), G::default(), H::default(), I::default(), J::default()) } }
impl<A: Default, B: Default, C: Default, D: Default, E: Default, F: Default, G: Default, H: Default, I: Default, J: Default, K: Default> Default for (A, B, C, D, E, F, G, H, I, J, K) { fn default() -> Self { (A::default(), B::default(), C::default(), D::default(), E::default(), F::default(), G::default(), H::default(), I::default(), J::default(), K::default()) } }
impl<A: Default, B: Default, C: Default, D: Default, E: Default, F: Default, G: Default, H: Default, I: Default, J: Default, K: Default, L: Default> Default for (A, B, C, D, E, F, G, H, I, J, K, L) { fn default() -> Self { (A::default(), B::default(), C::default(), D::default(), E::default(), F::default(), G::default(), H::default(), I::default(), J::default(), K::default(), L::default()) } }
