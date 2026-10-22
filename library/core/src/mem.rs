//! Functions on values in general.

/// Disposes of a value: moves it into a call that does nothing with it.
pub fn drop<T>(_x: T) {}

/// Takes ownership of `t` without dropping it: its `Drop` impl never
/// runs, and what it owns is leaked.
pub const fn forget<T>(t: T) {
    let _ = ManuallyDrop::new(t);
}

/// Whether dropping a `T` runs code: a `Drop` impl of it or of a part.
pub const fn needs_drop<T>() -> bool {
    crate::intrinsics::needs_drop::<T>()
}

/// The size of a `T` in bytes, as rustc lays it out: its fields
/// reordered to leave little padding, an enum's discriminant in the
/// smallest integer that holds them or in an invalid value of a field
/// (`Option<&T>` is one pointer).
pub const fn size_of<T>() -> usize {
    crate::intrinsics::size_of::<T>()
}

/// The alignment of a `T` in bytes.
pub const fn align_of<T>() -> usize {
    crate::intrinsics::align_of::<T>()
}

/// The size of the value `val` refers to: of a slice's elements, or of
/// a trait object's type.
pub const fn size_of_val<T: ?Sized>(val: &T) -> usize {
    unsafe { crate::intrinsics::size_of_val(val as *const T) }
}

/// The alignment of the value `val` refers to.
pub const fn align_of_val<T: ?Sized>(val: &T) -> usize {
    unsafe { crate::intrinsics::align_of_val(val as *const T) }
}

/// A value that is not dropped: dropping a `ManuallyDrop<T>` does
/// nothing, whatever `T` is.
#[lang = "manually_drop"]
pub struct ManuallyDrop<T> {
    pub(crate) value: T,
}

impl<T: crate::marker::Copy> crate::clone::Clone for ManuallyDrop<T> {
    fn clone(&self) -> ManuallyDrop<T> {
        *self
    }
}

impl<T: crate::marker::Copy> crate::marker::Copy for ManuallyDrop<T> {}

impl<T> crate::ops::Deref for ManuallyDrop<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> crate::ops::DerefMut for ManuallyDrop<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
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

/// Memory for a `T` that may not hold one yet: never dropped, and with
/// no invalid values an enum around it could use, as rustc's union is.
#[lang = "maybe_uninit"]
pub struct MaybeUninit<T> {
    value: ManuallyDrop<T>,
}

impl<T: crate::marker::Copy> crate::clone::Clone for MaybeUninit<T> {
    fn clone(&self) -> MaybeUninit<T> {
        *self
    }
}

impl<T: crate::marker::Copy> crate::marker::Copy for MaybeUninit<T> {}

impl<T> MaybeUninit<T> {
    /// Holding `val`.
    pub const fn new(val: T) -> MaybeUninit<T> {
        MaybeUninit {
            value: ManuallyDrop::new(val),
        }
    }

    /// Holding nothing yet.
    pub const fn uninit() -> MaybeUninit<T> {
        MaybeUninit {
            value: ManuallyDrop::new(unsafe { crate::intrinsics::uninit() }),
        }
    }

    /// Holding a `T` whose bytes are all zero.
    pub fn zeroed() -> MaybeUninit<T> {
        let mut u = MaybeUninit::uninit();
        unsafe { crate::ptr::write_bytes(u.as_mut_ptr(), 0, 1) };
        u
    }

    /// Puts `val` in, without dropping what was there, and gives a
    /// reference to it.
    pub fn write(&mut self, val: T) -> &mut T {
        unsafe {
            crate::intrinsics::write(&mut self.value.value, val);
        }
        &mut self.value.value
    }

    /// A pointer to the value.
    pub const fn as_ptr(&self) -> *const T {
        &self.value.value as *const T
    }

    /// A mutable pointer to the value.
    pub fn as_mut_ptr(&mut self) -> *mut T {
        &mut self.value.value as *mut T
    }

    /// The value, which must have been written.
    pub unsafe fn assume_init(self) -> T {
        ManuallyDrop::into_inner(self.value)
    }

    /// A copy of the value, which must have been written; the caller
    /// makes sure only one is dropped.
    pub unsafe fn assume_init_read(&self) -> T {
        unsafe { crate::intrinsics::read(&self.value.value) }
    }

    /// A reference to the value, which must have been written.
    pub unsafe fn assume_init_ref(&self) -> &T {
        &self.value.value
    }

    /// A mutable reference to the value, which must have been written.
    pub unsafe fn assume_init_mut(&mut self) -> &mut T {
        &mut self.value.value
    }

    /// Drops the value, which must have been written.
    pub unsafe fn assume_init_drop(&mut self) {
        unsafe { crate::ptr::drop_in_place(self.as_mut_ptr()) }
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

/// Takes the value `dest` refers to, leaving its type's default.
#[rustc_const_unstable]
pub fn take<T: crate::default::Default>(dest: &mut T) -> T {
    replace(dest, T::default())
}
