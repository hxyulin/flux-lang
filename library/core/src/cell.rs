//! Shareable mutable containers: values that may change behind a shared
//! reference. A `Cell` gives its value out by copy and takes new ones; a
//! `RefCell` lends it out, checking at run time that a mutable loan is
//! the only one, and panics otherwise, as rustc's does.
//!
//! Both write through a pointer made from a shared reference, which
//! rustc allows only through `UnsafeCell`; flux's code generation
//! assumes nothing of shared references that this breaks.

use crate::clone::Clone;
use crate::cmp::{Eq, Ord, Ordering, PartialEq, PartialOrd};
use crate::convert::From;
use crate::default::Default;
use crate::fmt::{self, Debug, Display, Formatter};
use crate::marker::Copy;
use crate::mem;
use crate::ops::{Drop, FnOnce};
use crate::option::Option::{self, None, Some};
use crate::panicking::panic;
use crate::result::Result::{self, Err, Ok};

/// A mutable place behind `value`, which a shared reference reaches.
fn as_mut<T>(value: &T) -> &mut T {
    unsafe { &mut *(value as *const T as *mut T) }
}

/// A value that may be replaced through a shared reference.
pub struct Cell<T> {
    value: T,
}

impl<T> Cell<T> {
    /// A cell holding `value`.
    pub const fn new(value: T) -> Cell<T> {
        Cell { value }
    }

    /// Stores `val`, dropping the old value.
    pub fn set(&self, val: T) {
        mem::drop(self.replace(val));
    }

    /// Stores `val`, returning the old value.
    pub fn replace(&self, val: T) -> T {
        mem::replace(as_mut(&self.value), val)
    }

    /// Swaps the values of two cells.
    pub fn swap(&self, other: &Cell<T>) {
        if self as *const Cell<T> as usize == other as *const Cell<T> as usize {
            return;
        }
        mem::swap(as_mut(&self.value), as_mut(&other.value));
    }

    /// The value, taking the cell.
    pub fn into_inner(self) -> T {
        self.value
    }

    /// The value, mutably: the cell is borrowed mutably, so no one else
    /// sees it.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// A pointer to the value.
    pub const fn as_ptr(&self) -> *mut T {
        &self.value as *const T as *mut T
    }
}

impl<T: Copy> Cell<T> {
    /// A copy of the value.
    pub fn get(&self) -> T {
        self.value
    }

    /// Replaces the value with `f` of it.
    pub fn update<F: FnOnce(T) -> T>(&self, f: F) {
        let old = self.get();
        self.set(f(old));
    }
}

impl<T: Default> Cell<T> {
    /// The value, leaving the default in its place.
    pub fn take(&self) -> T {
        self.replace(T::default())
    }
}

impl<T: Copy> Clone for Cell<T> {
    fn clone(&self) -> Cell<T> {
        Cell::new(self.get())
    }
}

impl<T: Default> Default for Cell<T> {
    fn default() -> Cell<T> {
        Cell::new(T::default())
    }
}

impl<T: PartialEq + Copy> PartialEq for Cell<T> {
    fn eq(&self, other: &Cell<T>) -> bool {
        self.get() == other.get()
    }
}

impl<T: Eq + Copy> Eq for Cell<T> {}

impl<T: PartialOrd + Copy> PartialOrd for Cell<T> {
    fn partial_cmp(&self, other: &Cell<T>) -> Option<Ordering> {
        self.get().partial_cmp(&other.get())
    }
}

impl<T: Ord + Copy> Ord for Cell<T> {
    fn cmp(&self, other: &Cell<T>) -> Ordering {
        self.get().cmp(&other.get())
    }
}

impl<T> From<T> for Cell<T> {
    fn from(t: T) -> Cell<T> {
        Cell::new(t)
    }
}

/// What `RefCell::try_borrow` gives where the value is lent mutably.
pub struct BorrowError {
    _private: (),
}

impl Debug for BorrowError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("BorrowError")
    }
}

impl Display for BorrowError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.pad("RefCell already mutably borrowed")
    }
}

/// What `RefCell::try_borrow_mut` gives where the value is lent.
pub struct BorrowMutError {
    _private: (),
}

impl Debug for BorrowMutError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("BorrowMutError")
    }
}

impl Display for BorrowMutError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.pad("RefCell already borrowed")
    }
}

/// `Cell { value: 5 }`, as rustc's.
impl<T: Copy + Debug> Debug for Cell<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cell").field("value", &self.get()).finish()
    }
}

/// What a `RefCell` lent mutably shows for its value.
struct BorrowedPlaceholder;

impl Debug for BorrowedPlaceholder {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("<borrowed>")
    }
}

/// `RefCell { value: 7 }`, or `RefCell { value: <borrowed> }` while it
/// is lent mutably, as rustc's.
impl<T: Debug> Debug for RefCell<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("RefCell");
        match self.try_borrow() {
            Ok(borrow) => d.field("value", borrow.value),
            Err(_) => d.field("value", &BorrowedPlaceholder),
        };
        d.finish()
    }
}

/// A value lent out through shared references, checked at run time:
/// any number of `Ref`s, or one `RefMut`.
pub struct RefCell<T> {
    /// How many `Ref`s there are, or -1 for a `RefMut`.
    borrow: Cell<isize>,
    value: T,
}

impl<T> RefCell<T> {
    /// A cell holding `value`.
    pub const fn new(value: T) -> RefCell<T> {
        RefCell { borrow: Cell::new(0), value }
    }

    /// The value, taking the cell.
    pub fn into_inner(self) -> T {
        self.value
    }

    /// A shared loan of the value; panics if it is lent mutably.
    #[track_caller]
    pub fn borrow(&self) -> Ref<'_, T> {
        match self.try_borrow() {
            Ok(r) => r,
            Err(_) => panic("RefCell already mutably borrowed"),
        }
    }

    /// A shared loan of the value, unless it is lent mutably.
    pub fn try_borrow(&self) -> Result<Ref<'_, T>, BorrowError> {
        let b = self.borrow.get();
        if b < 0 {
            return Err(BorrowError { _private: () });
        }
        self.borrow.set(b + 1);
        Ok(Ref { value: &self.value, borrow: &self.borrow })
    }

    /// A mutable loan of the value; panics if it is lent.
    #[track_caller]
    pub fn borrow_mut(&self) -> RefMut<'_, T> {
        match self.try_borrow_mut() {
            Ok(r) => r,
            Err(_) => panic("RefCell already borrowed"),
        }
    }

    /// A mutable loan of the value, unless it is lent.
    pub fn try_borrow_mut(&self) -> Result<RefMut<'_, T>, BorrowMutError> {
        if self.borrow.get() != 0 {
            return Err(BorrowMutError { _private: () });
        }
        self.borrow.set(-1);
        Ok(RefMut { value: as_mut(&self.value), borrow: &self.borrow })
    }

    /// Stores `t`, returning the old value; panics if it is lent.
    #[track_caller]
    pub fn replace(&self, t: T) -> T {
        let mut r = self.borrow_mut();
        mem::replace(r.value_mut(), t)
    }

    /// Replaces the value with `f` of it, returning the old one; panics
    /// if it is lent.
    #[track_caller]
    pub fn replace_with<F: FnOnce(&mut T) -> T>(&self, f: F) -> T {
        let mut r = self.borrow_mut();
        let new = f(r.value_mut());
        mem::replace(r.value_mut(), new)
    }

    /// Swaps the values of two cells; panics if either is lent.
    #[track_caller]
    pub fn swap(&self, other: &RefCell<T>) {
        let mut a = self.borrow_mut();
        let mut b = other.borrow_mut();
        mem::swap(a.value_mut(), b.value_mut());
    }

    /// The value, mutably: the cell is borrowed mutably, so no one else
    /// sees it.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// A pointer to the value.
    pub fn as_ptr(&self) -> *mut T {
        &self.value as *const T as *mut T
    }
}

impl<T: Default> RefCell<T> {
    /// The value, leaving the default in its place; panics if it is
    /// lent.
    #[track_caller]
    pub fn take(&self) -> T {
        self.replace(T::default())
    }
}

impl<T: Clone> Clone for RefCell<T> {
    /// Panics if the value is lent mutably.
    #[track_caller]
    fn clone(&self) -> RefCell<T> {
        let r = self.borrow();
        RefCell::new(r.value.clone())
    }
}

impl<T: Default> Default for RefCell<T> {
    fn default() -> RefCell<T> {
        RefCell::new(T::default())
    }
}

impl<T: PartialEq> PartialEq for RefCell<T> {
    /// Panics if either value is lent mutably.
    #[track_caller]
    fn eq(&self, other: &RefCell<T>) -> bool {
        let (a, b) = (self.borrow(), other.borrow());
        *a.value == *b.value
    }
}

impl<T: Eq> Eq for RefCell<T> {}

impl<T> From<T> for RefCell<T> {
    fn from(t: T) -> RefCell<T> {
        RefCell::new(t)
    }
}

/// A shared loan of a `RefCell`'s value, given back when dropped.
pub struct Ref<'b, T> {
    value: &'b T,
    borrow: &'b Cell<isize>,
}

impl<'b, T> Ref<'b, T> {
    /// Another loan of the same value.
    pub fn clone(orig: &Ref<'b, T>) -> Ref<'b, T> {
        orig.borrow.set(orig.borrow.get() + 1);
        Ref { value: orig.value, borrow: orig.borrow }
    }
}

/// The value's.
impl<'b, T: Debug> Debug for Ref<'b, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl<'b, T> Drop for Ref<'b, T> {
    fn drop(&mut self) {
        self.borrow.set(self.borrow.get() - 1);
    }
}

/// A mutable loan of a `RefCell`'s value, given back when dropped.
pub struct RefMut<'b, T> {
    value: &'b mut T,
    borrow: &'b Cell<isize>,
}

impl<'b, T> RefMut<'b, T> {
    fn value_mut(&mut self) -> &mut T {
        &mut *self.value
    }
}

/// The value's.
impl<'b, T: Debug> Debug for RefMut<'b, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        (*self.value).fmt(f)
    }
}

impl<'b, T> Drop for RefMut<'b, T> {
    fn drop(&mut self) {
        self.borrow.set(0);
    }
}
