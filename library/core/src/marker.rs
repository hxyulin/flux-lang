//! Traits that say what kind of type a type is.

/// Types whose size is known at compile time: every type flux has. A
/// trait's `Self` is not assumed to be, so a provided method that takes
/// `self` by value needs `where Self: Sized`.
#[lang = "sized"]
pub trait Sized {}
