//! Borrowed forms with equivalent equality, ordering and hashing.

/// A reference to an equivalent borrowed form of a value.
pub trait Borrow<Borrowed: ?Sized> {
    fn borrow(&self) -> &Borrowed;
}

/// A mutable reference to an equivalent borrowed form of a value.
pub trait BorrowMut<Borrowed: ?Sized>: Borrow<Borrowed> {
    fn borrow_mut(&mut self) -> &mut Borrowed;
}

impl<T: ?Sized> Borrow<T> for T {
    fn borrow(&self) -> &T {
        self
    }
}

impl<T: ?Sized> BorrowMut<T> for T {
    fn borrow_mut(&mut self) -> &mut T {
        self
    }
}

impl<T: ?Sized> Borrow<T> for &T {
    fn borrow(&self) -> &T {
        *self
    }
}

impl<T: ?Sized> Borrow<T> for &mut T {
    fn borrow(&self) -> &T {
        *self
    }
}

impl<T: ?Sized> BorrowMut<T> for &mut T {
    fn borrow_mut(&mut self) -> &mut T {
        *self
    }
}
