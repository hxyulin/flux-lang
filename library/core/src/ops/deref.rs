//! Dereferencing: `*v` on a type other than a reference is
//! `*Deref::deref(&v)`, or `*DerefMut::deref_mut(&mut v)` where the place
//! is assigned to or borrowed mutably, as in rustc. Method calls, field
//! accesses and indexing dereference through these traits as they do
//! through references, and a `&W` coerces to a `&W::Target` where one is
//! expected.

/// What `*v` is, when it is read.
#[lang = "deref"]
pub trait Deref {
    /// What a value dereferences to: `[T]`, `str` and trait objects may
    /// be (`?Sized` in rustc; flux does not check associated types'
    /// sizes).
    type Target;

    fn deref(&self) -> &Self::Target;
}

/// What `*v` is, when it is assigned to or borrowed mutably.
#[lang = "deref_mut"]
pub trait DerefMut: Deref {
    fn deref_mut(&mut self) -> &mut Self::Target;
}

impl<T: ?Sized> Deref for &T {
    type Target = T;

    fn deref(&self) -> &T {
        *self
    }
}

impl<T: ?Sized> Deref for &mut T {
    type Target = T;

    fn deref(&self) -> &T {
        &**self
    }
}

impl<T: ?Sized> DerefMut for &mut T {
    fn deref_mut(&mut self) -> &mut T {
        &mut **self
    }
}
