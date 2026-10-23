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

/// Types whose ownership may be transferred between threads.
/// The compiler propagates this auto trait through fields unless an
/// explicit unsafe implementation supplies its own requirements.
#[lang = "send"]
pub unsafe trait Send {}

/// Types whose shared references may be transferred between threads.
#[lang = "sync"]
pub unsafe trait Sync {}

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
impl<T: ?Sized> Copy for *const T {}
impl<T: ?Sized> Copy for *mut T {}
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

/// A type that acts as if it held a `T`, while holding nothing: for
/// structs whose parameters no field names. It is covariant in `T`.
#[lang = "phantom_data"]
pub struct PhantomData<T: ?Sized>;

impl<T: ?Sized> Clone for PhantomData<T> {
    fn clone(&self) -> PhantomData<T> {
        PhantomData
    }
}

impl<T: ?Sized> Copy for PhantomData<T> {}

/// It hashes nothing.
impl<T: ?Sized> crate::hash::Hash for PhantomData<T> {
    fn hash<H: crate::hash::Hasher>(&self, _state: &mut H) {}
}

// Marker values carry no runtime state, regardless of the marked type.
impl<T: ?Sized> crate::cmp::PartialEq for PhantomData<T> {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl<T: ?Sized> crate::cmp::Eq for PhantomData<T> {}

impl<T: ?Sized> crate::cmp::PartialOrd for PhantomData<T> {
    fn partial_cmp(&self, _other: &Self) -> crate::option::Option<crate::cmp::Ordering> {
        crate::option::Option::Some(crate::cmp::Ordering::Equal)
    }
}

impl<T: ?Sized> crate::cmp::Ord for PhantomData<T> {
    fn cmp(&self, _other: &Self) -> crate::cmp::Ordering {
        crate::cmp::Ordering::Equal
    }
}
