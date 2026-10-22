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

/// Swaps the values `x` and `y` refer to.
pub fn swap<T>(x: &mut T, y: &mut T) {
    // Each value is read out once and written back once, to the other
    // place, so none is dropped or duplicated.
    unsafe {
        let a = crate::intrinsics::read(x);
        let b = crate::intrinsics::read(y);
        crate::intrinsics::write(x, b);
        crate::intrinsics::write(y, a);
    }
}

/// Puts `src` where `dest` refers, and gives back what was there.
pub fn replace<T>(dest: &mut T, src: T) -> T {
    let mut src = src;
    swap(dest, &mut src);
    src
}
