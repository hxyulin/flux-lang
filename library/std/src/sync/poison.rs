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

/// Why a non-blocking lock attempt failed.
pub enum TryLockError<T> {
    Poisoned(PoisonError<T>),
    WouldBlock,
}
pub type TryLockResult<Guard> = core::result::Result<Guard, TryLockError<Guard>>;
impl<T> From<PoisonError<T>> for TryLockError<T> {
    fn from(error: PoisonError<T>) -> TryLockError<T> { TryLockError::Poisoned(error) }
}
impl<T> Debug for TryLockError<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            TryLockError::Poisoned(..) => f.write_str("Poisoned(..)"),
            TryLockError::WouldBlock => f.write_str("WouldBlock"),
        }
    }
}
impl<T> Display for TryLockError<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            TryLockError::Poisoned(..) => f.write_str("poisoned lock: another task failed inside"),
            TryLockError::WouldBlock => f.write_str("try_lock failed because the operation would block"),
        }
    }
}
impl<T> core::error::Error for TryLockError<T> {}
