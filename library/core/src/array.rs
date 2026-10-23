//! Arrays' traits and methods, and their iterator by value, written
//! with const generics as rustc's `core` writes them.

use crate::clone::Clone;
use crate::cmp::{Eq, Ord, Ordering, PartialEq, PartialOrd};
use crate::convert::TryFrom;
use crate::default::Default;
use crate::fmt::{Debug, Formatter, Result};
use crate::intrinsics::{is_copy, read, uninit, write};
use crate::iter::{DoubleEndedIterator, ExactSizeIterator, IntoIterator, Iterator};
use crate::marker::Copy;
use crate::mem::{ManuallyDrop, drop};
use crate::ops::{Drop, FnMut};
use crate::option::Option::{self, None, Some};
use crate::slice::{Iter, IterMut};

/// The slice's length did not match the array's length.
#[derive(Clone, Copy, Debug)]
pub struct TryFromSliceError(());

impl crate::fmt::Display for TryFromSliceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("could not convert slice to array")
    }
}

impl<T: Copy, const N: usize> TryFrom<&[T]> for [T; N] {
    type Error = TryFromSliceError;

    fn try_from(slice: &[T]) -> crate::result::Result<[T; N], TryFromSliceError> {
        if slice.len() == N {
            crate::result::Result::Ok(from_fn(|i| slice[i]))
        } else {
            crate::result::Result::Err(TryFromSliceError(()))
        }
    }
}

/// An array of `N` elements, each `cb(i)` for its index `i`, made in
/// order.
pub fn from_fn<T, const N: usize, F: FnMut(usize) -> T>(mut cb: F) -> [T; N] {
    // Each element is written once before the array is used.
    let mut array: ManuallyDrop<[T; N]> = ManuallyDrop::new(unsafe { uninit() });
    let mut i = 0;
    while i < N {
        unsafe { write(&mut array.value[i], cb(i)) };
        i += 1;
    }
    ManuallyDrop::into_inner(array)
}

impl<T, const N: usize> [T; N] {
    /// An array of `f` applied to each element, in order.
    pub fn map<U, F: FnMut(T) -> U>(self, mut f: F) -> [U; N] {
        let mut iter = self.into_iter();
        from_fn(|_| match iter.next() {
            Some(item) => f(item),
            None => loop {},
        })
    }

    /// The array as a slice: `&self[..]`.
    pub const fn as_slice(&self) -> &[T] {
        self
    }

    /// The array as a mutable slice.
    pub const fn as_mut_slice(&mut self) -> &mut [T] {
        self
    }

    /// An array of references to each element.
    pub fn each_ref(&self) -> [&T; N] {
        from_fn(|i| &self[i])
    }
}

/// An iterator over an array's elements by value: what `for x in array`
/// and `array.into_iter()` give. Dropping it drops the elements it has
/// not given.
pub struct IntoIter<T, const N: usize> {
    data: ManuallyDrop<[T; N]>,
    start: usize,
    end: usize,
}

impl<T, const N: usize> IntoIter<T, N> {
    /// The elements not given yet.
    pub fn as_slice(&self) -> &[T] {
        &self.data.value[self.start..self.end]
    }
}

impl<T, const N: usize> IntoIterator for [T; N] {
    type Item = T;
    type IntoIter = IntoIter<T, N>;
    const __CONST_SIZE: Option<usize> = Some(N);

    fn into_iter(self) -> IntoIter<T, N> {
        IntoIter {
            data: ManuallyDrop::new(self),
            start: 0,
            end: N,
        }
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a [T; N] {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;
    const __CONST_SIZE: Option<usize> = Some(N);

    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a mut [T; N] {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;
    const __CONST_SIZE: Option<usize> = Some(N);

    fn into_iter(self) -> IterMut<'a, T> {
        self.iter_mut()
    }
}

impl<T, const N: usize> Iterator for IntoIter<T, N> {
    type Item = T;
    const __TRUSTED_LEN: bool = true;

    fn next(&mut self) -> Option<T> {
        if self.start < self.end {
            let item = unsafe { read(&self.data.value[self.start]) };
            self.start += 1;
            Some(item)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.end - self.start;
        (n, Some(n))
    }
}

impl<T, const N: usize> ExactSizeIterator for IntoIter<T, N> {}

impl<T, const N: usize> DoubleEndedIterator for IntoIter<T, N> {
    fn next_back(&mut self) -> Option<T> {
        if self.start < self.end {
            self.end -= 1;
            Some(unsafe { read(&self.data.value[self.end]) })
        } else {
            None
        }
    }

    /// As rustc's (through `advance_back_by`): the `n` elements skipped
    /// are dropped as one slice, front to back, before the one returned.
    fn nth_back(&mut self, n: usize) -> Option<T> {
        let left = self.end - self.start;
        let step = if n < left { n } else { left };
        let mut i = self.end - step;
        let end = self.end;
        self.end -= step;
        while i < end {
            drop(unsafe { read(&self.data.value[i]) });
            i += 1;
        }
        self.next_back()
    }
}

impl<T, const N: usize> Drop for IntoIter<T, N> {
    fn drop(&mut self) {
        while self.start < self.end {
            drop(unsafe { read(&self.data.value[self.start]) });
            self.start += 1;
        }
    }
}

impl<T: Copy, const N: usize> Copy for [T; N] {}

/// A copy if `T` is `Copy`, as rustc's specialization makes it, else
/// each element's clone in turn.
impl<T: Clone, const N: usize> Clone for [T; N] {
    fn clone(&self) -> [T; N] {
        if is_copy::<T>() {
            unsafe { read(self) }
        } else {
            from_fn(|i| self[i].clone())
        }
    }
}

impl<A: PartialEq<B>, B, const N: usize> PartialEq<[B; N]> for [A; N] {
    fn eq(&self, other: &[B; N]) -> bool {
        self[..] == other[..]
    }

    fn ne(&self, other: &[B; N]) -> bool {
        self[..] != other[..]
    }
}

impl<A: PartialEq<B>, B, const N: usize> PartialEq<[B]> for [A; N] {
    fn eq(&self, other: &[B]) -> bool {
        self[..] == other[..]
    }

    fn ne(&self, other: &[B]) -> bool {
        self[..] != other[..]
    }
}

impl<A, B: PartialEq<A>, const N: usize> PartialEq<[A; N]> for [B] {
    fn eq(&self, other: &[A; N]) -> bool {
        self[..] == other[..]
    }

    fn ne(&self, other: &[A; N]) -> bool {
        self[..] != other[..]
    }
}

impl<A: PartialEq<B>, B, const N: usize> PartialEq<&[B]> for [A; N] {
    fn eq(&self, other: &&[B]) -> bool {
        self[..] == other[..]
    }

    fn ne(&self, other: &&[B]) -> bool {
        self[..] != other[..]
    }
}

impl<A, B: PartialEq<A>, const N: usize> PartialEq<[A; N]> for &[B] {
    fn eq(&self, other: &[A; N]) -> bool {
        self[..] == other[..]
    }

    fn ne(&self, other: &[A; N]) -> bool {
        self[..] != other[..]
    }
}

impl<A: PartialEq<B>, B, const N: usize> PartialEq<&mut [B]> for [A; N] {
    fn eq(&self, other: &&mut [B]) -> bool {
        self[..] == other[..]
    }

    fn ne(&self, other: &&mut [B]) -> bool {
        self[..] != other[..]
    }
}

impl<A, B: PartialEq<A>, const N: usize> PartialEq<[A; N]> for &mut [B] {
    fn eq(&self, other: &[A; N]) -> bool {
        self[..] == other[..]
    }

    fn ne(&self, other: &[A; N]) -> bool {
        self[..] != other[..]
    }
}

impl<T: Eq, const N: usize> Eq for [T; N] {}

/// Arrays compare as slices do.
impl<T: PartialOrd, const N: usize> PartialOrd for [T; N] {
    fn partial_cmp(&self, other: &[T; N]) -> Option<Ordering> {
        PartialOrd::partial_cmp(&&self[..], &&other[..])
    }
}

impl<T: Ord, const N: usize> Ord for [T; N] {
    fn cmp(&self, other: &[T; N]) -> Ordering {
        Ord::cmp(&&self[..], &&other[..])
    }
}

impl<T: Debug, const N: usize> Debug for [T; N] {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        Debug::fmt(&&self[..], f)
    }
}

// `Default` for arrays of up to 32 elements, as rustc's `core` has it:
// those of none for any element type.
impl<T> Default for [T; 0] {
    fn default() -> [T; 0] {
        []
    }
}
impl<T: Default> Default for [T; 1] { fn default() -> [T; 1] { [T::default()] } }
impl<T: Default> Default for [T; 2] { fn default() -> [T; 2] { [T::default(), T::default()] } }
impl<T: Default> Default for [T; 3] { fn default() -> [T; 3] { [T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 4] { fn default() -> [T; 4] { [T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 5] { fn default() -> [T; 5] { [T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 6] { fn default() -> [T; 6] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 7] { fn default() -> [T; 7] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 8] { fn default() -> [T; 8] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 9] { fn default() -> [T; 9] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 10] { fn default() -> [T; 10] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 11] { fn default() -> [T; 11] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 12] { fn default() -> [T; 12] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 13] { fn default() -> [T; 13] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 14] { fn default() -> [T; 14] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 15] { fn default() -> [T; 15] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 16] { fn default() -> [T; 16] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 17] { fn default() -> [T; 17] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 18] { fn default() -> [T; 18] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 19] { fn default() -> [T; 19] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 20] { fn default() -> [T; 20] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 21] { fn default() -> [T; 21] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 22] { fn default() -> [T; 22] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 23] { fn default() -> [T; 23] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 24] { fn default() -> [T; 24] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 25] { fn default() -> [T; 25] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 26] { fn default() -> [T; 26] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 27] { fn default() -> [T; 27] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 28] { fn default() -> [T; 28] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 29] { fn default() -> [T; 29] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 30] { fn default() -> [T; 30] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 31] { fn default() -> [T; 31] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
impl<T: Default> Default for [T; 32] { fn default() -> [T; 32] { [T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default(), T::default()] } }
