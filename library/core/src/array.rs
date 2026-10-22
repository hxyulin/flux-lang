//! Arrays' iterator by value. `IntoIterator` for `[T; N]`, `&[T; N]` and
//! `&mut [T; N]` is built into the compiler, as rustc's `core` writes it
//! with const generics, which flux does not have: its `into_iter` is
//! [`IntoIter::new`], [`iter`] or [`iter_mut`] here, with `A` the
//! array's type.

use crate::intrinsics::{array_as_mut_slice, array_as_slice, read, subslice};
use crate::iter::{DoubleEndedIterator, Iterator};
use crate::marker::PhantomData;
use crate::mem::{ManuallyDrop, drop};
use crate::ops::Drop;
use crate::option::Option::{self, None, Some};
use crate::slice::{Iter, IterMut};

/// An iterator over an array's elements by value: what `for x in array`
/// and `array.into_iter()` give. Dropping it drops the elements it has
/// not given. rustc's is `IntoIter<T, N>`; flux's has the array's type,
/// `[T; N]`, in place of `N`.
#[lang = "array_into_iter"]
pub struct IntoIter<T, A> {
    data: ManuallyDrop<A>,
    start: usize,
    end: usize,
    elem: PhantomData<T>,
}

impl<T, A> IntoIter<T, A> {
    /// `IntoIterator::into_iter` of an array of type `A`, `[T; N]`.
    #[lang = "array_into_iter_new"]
    pub(crate) fn new(array: A) -> IntoIter<T, A> {
        let end = unsafe { array_as_slice::<A, T>(&array) }.len();
        IntoIter {
            data: ManuallyDrop::new(array),
            start: 0,
            end,
            elem: PhantomData,
        }
    }

    /// The elements not given yet.
    pub fn as_slice(&self) -> &[T] {
        unsafe { subslice(array_as_slice::<A, T>(&self.data.value), self.start, self.end) }
    }
}

impl<T, A> Iterator for IntoIter<T, A> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        if self.start < self.end {
            let all = unsafe { array_as_slice::<A, T>(&self.data.value) };
            let item = unsafe { read(&all[self.start]) };
            self.start += 1;
            Some(item)
        } else {
            None
        }
    }
}

impl<T, A> DoubleEndedIterator for IntoIter<T, A> {
    fn next_back(&mut self) -> Option<T> {
        if self.start < self.end {
            self.end -= 1;
            let all = unsafe { array_as_slice::<A, T>(&self.data.value) };
            Some(unsafe { read(&all[self.end]) })
        } else {
            None
        }
    }
}

impl<T, A> Drop for IntoIter<T, A> {
    #[lang = "array_into_iter_drop"]
    fn drop(&mut self) {
        let all = unsafe { array_as_mut_slice::<A, T>(&mut self.data.value) };
        while self.start < self.end {
            drop(unsafe { read(&all[self.start]) });
            self.start += 1;
        }
    }
}

/// `IntoIterator::into_iter` of a `&[T; N]`, of type `&A`.
#[lang = "array_iter"]
pub(crate) fn iter<T, A>(array: &A) -> Iter<'_, T> {
    unsafe { array_as_slice::<A, T>(array) }.iter()
}

/// `IntoIterator::into_iter` of a `&mut [T; N]`, of type `&mut A`.
#[lang = "array_iter_mut"]
pub(crate) fn iter_mut<T, A>(array: &mut A) -> IterMut<'_, T> {
    unsafe { array_as_mut_slice::<A, T>(array) }.iter_mut()
}
