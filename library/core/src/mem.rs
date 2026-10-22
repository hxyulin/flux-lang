//! Functions on values in general.

/// Disposes of a value: moves it into a call that does nothing with it.
pub fn drop<T>(_x: T) {}

/// A value that is not dropped: dropping a `ManuallyDrop<T>` does
/// nothing, whatever `T` is.
#[lang = "manually_drop"]
pub struct ManuallyDrop<T> {
    pub(crate) value: T,
}

impl<T> ManuallyDrop<T> {
    /// Wraps `value`, which will not be dropped.
    pub const fn new(value: T) -> ManuallyDrop<T> {
        ManuallyDrop { value }
    }

    /// The value, which will be dropped as usual again.
    pub const fn into_inner(slot: ManuallyDrop<T>) -> T {
        slot.value
    }
}
