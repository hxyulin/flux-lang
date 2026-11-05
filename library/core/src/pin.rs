//! Pointer wrappers that preserve the location of pinned values.
use crate::marker::Unpin;
use crate::ops::{Deref, DerefMut};

#[lang = "pin"]
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Pin<Ptr> { pointer: Ptr }

impl<Ptr: Deref> Pin<Ptr> {
    pub fn new(pointer: Ptr) -> Self where Ptr::Target: Unpin {
        // SAFETY: Unpin permits moving the target at any time.
        unsafe { Self::new_unchecked(pointer) }
    }
    /// # Safety
    /// The pointer must keep its target pinned until the target is dropped.
    pub const unsafe fn new_unchecked(pointer: Ptr) -> Self { Self { pointer } }
    pub fn into_inner(pin: Self) -> Ptr where Ptr::Target: Unpin { pin.pointer }
    /// # Safety
    /// The target must remain pinned until it is dropped, even after this call.
    pub unsafe fn into_inner_unchecked(pin: Self) -> Ptr { pin.pointer }
    pub fn as_ref(&self) -> Pin<&Ptr::Target> {
        // SAFETY: this shared reborrow cannot move the pinned target.
        unsafe { Pin::new_unchecked(&*self.pointer) }
    }
}
impl<Ptr: DerefMut> Pin<Ptr> {
    pub fn as_mut(&mut self) -> Pin<&mut Ptr::Target> {
        // SAFETY: this exclusive reborrow retains the owner's pin guarantee.
        unsafe { Pin::new_unchecked(&mut *self.pointer) }
    }
    pub fn set(&mut self, value: Ptr::Target) where Ptr::Target: Sized {
        // Assignment destroys the old value in place before replacing it.
        *self.pointer = value;
    }
}
impl<'a,T:?Sized> Pin<&'a T> {
    pub const fn get_ref(self) -> &'a T { self.pointer }
    /// # Safety
    /// The returned reference must point to a structurally pinned part of T.
    pub unsafe fn map_unchecked<U:?Sized,F>(self, f:F) -> Pin<&'a U>
    where F:FnOnce(&T)->&U {
        // SAFETY: the caller promises that the projection preserves pinning.
        unsafe { Pin::new_unchecked(f(self.pointer)) }
    }
}
impl<'a,T:?Sized> Pin<&'a mut T> {
    pub fn get_mut(self) -> &'a mut T where T:Unpin { self.pointer }
    /// # Safety
    /// The target must not be moved through the returned reference.
    pub const unsafe fn get_unchecked_mut(self) -> &'a mut T { self.pointer }
    pub fn into_ref(self) -> Pin<&'a T> {
        // SAFETY: weakening exclusivity cannot move the pinned value.
        unsafe { Pin::new_unchecked(self.pointer) }
    }
    /// # Safety
    /// The closure must not move the target and must return a structurally
    /// pinned part that remains pinned until the original target is dropped.
    pub unsafe fn map_unchecked_mut<U:?Sized,F>(self, f:F) -> Pin<&'a mut U>
    where F:FnOnce(&mut T)->&mut U {
        // SAFETY: the caller supplies the projection's pinning contract.
        unsafe { Pin::new_unchecked(f(self.pointer)) }
    }
}
impl<Ptr:Deref> Deref for Pin<Ptr> {
    type Target=Ptr::Target;
    fn deref(&self)->&Self::Target { &*self.pointer }
}
impl<Ptr:DerefMut> DerefMut for Pin<Ptr> where Ptr::Target:Unpin {
    fn deref_mut(&mut self)->&mut Self::Target { &mut *self.pointer }
}

/// Pins a value in storage whose lifetime follows the surrounding expression.
#[rustc_builtin_macro]
#[macro_export]
macro_rules! pin { ($value:expr $(,)?) => {} }
pub use crate::pin;

impl<Ptr: crate::fmt::Debug> crate::fmt::Debug for Pin<Ptr> {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result { crate::fmt::Debug::fmt(&self.pointer, f) }
}
impl<Ptr: crate::fmt::Display> crate::fmt::Display for Pin<Ptr> {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result { crate::fmt::Display::fmt(&self.pointer, f) }
}
impl<Ptr: crate::fmt::Pointer> crate::fmt::Pointer for Pin<Ptr> {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result { crate::fmt::Pointer::fmt(&self.pointer, f) }
}
impl<Ptr: Deref, Q: Deref> crate::cmp::PartialEq<Pin<Q>> for Pin<Ptr>
where Ptr::Target: crate::cmp::PartialEq<Q::Target> {
    fn eq(&self, other: &Pin<Q>) -> bool { Ptr::Target::eq(&*self.pointer, &*other.pointer) }
}
impl<Ptr: Deref> crate::cmp::Eq for Pin<Ptr> where Ptr::Target: crate::cmp::Eq {}
impl<Ptr: Deref, Q: Deref> crate::cmp::PartialOrd<Pin<Q>> for Pin<Ptr>
where Ptr::Target: crate::cmp::PartialOrd<Q::Target> {
    fn partial_cmp(&self, other: &Pin<Q>) -> crate::option::Option<crate::cmp::Ordering> {
        Ptr::Target::partial_cmp(&*self.pointer, &*other.pointer)
    }
}
impl<Ptr: Deref> crate::cmp::Ord for Pin<Ptr> where Ptr::Target: crate::cmp::Ord {
    fn cmp(&self, other: &Self) -> crate::cmp::Ordering { Ptr::Target::cmp(&*self.pointer, &*other.pointer) }
}
impl<Ptr: Deref> crate::hash::Hash for Pin<Ptr> where Ptr::Target: crate::hash::Hash {
    fn hash<H: crate::hash::Hasher>(&self, state: &mut H) { Ptr::Target::hash(&*self.pointer, state) }
}
