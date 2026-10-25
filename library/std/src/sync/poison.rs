//! Poison errors shared by the bounded lock implementations.

use core::fmt::{Debug, Display, Formatter, Result};

pub type LockResult<T> = core::result::Result<T, PoisonError<T>>;

pub struct PoisonError<T> {
    value: T,
}

impl<T> PoisonError<T> {
    pub fn new(value: T) -> PoisonError<T> {
        PoisonError { value }
    }

    pub fn into_inner(self) -> T {
        self.value
    }

    pub fn get_ref(&self) -> &T {
        &self.value
    }

    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T> Debug for PoisonError<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("PoisonError { .. }")
    }
}

impl<T> Display for PoisonError<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("poisoned lock: another task failed inside")
    }
}
