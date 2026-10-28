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

/// An opaque identifier of an enum variant, independent of its fields.
pub struct Discriminant<T> {
    bits: u128,
    signed: bool,
    marker: crate::marker::PhantomData<fn() -> T>,
}

impl<T> crate::marker::Copy for Discriminant<T> {}
impl<T> crate::clone::Clone for Discriminant<T> {
    fn clone(&self) -> Self { *self }
}
impl<T> crate::cmp::PartialEq for Discriminant<T> {
    fn eq(&self, other: &Self) -> bool { self.bits == other.bits }
}
impl<T> crate::cmp::Eq for Discriminant<T> {}
impl<T> crate::hash::Hash for Discriminant<T> {
    fn hash<H: crate::hash::Hasher>(&self, state: &mut H) { crate::hash::Hash::hash(&self.bits, state); }
}
impl<T> crate::fmt::Debug for Discriminant<T> {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        if self.signed {
            crate::write!(f, "Discriminant({})", self.bits as i128)
        } else {
            crate::write!(f, "Discriminant({})", self.bits)
        }
    }
}

/// Returns the variant identity without inspecting or moving its fields.
pub const fn discriminant<T>(value: &T) -> Discriminant<T> {
    Discriminant {
        bits: crate::intrinsics::discriminant_value(value),
        signed: crate::intrinsics::discriminant_is_signed::<T>(),
        marker: crate::marker::PhantomData,
    }
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

/// A value whose bytes are all zero.
///
/// # Safety
/// All-zero bytes must represent a valid initialized `T`. In particular,
/// references, function pointers and nonzero integers do not permit zero.
pub unsafe fn zeroed<T>() -> T {
    // SAFETY: the caller guarantees the zeroed representation is valid.
    unsafe { MaybeUninit::<T>::zeroed().assume_init() }
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
            crate::intrinsics::write(&raw mut self.value.value, val);
        }
        &mut self.value.value
    }

    /// A pointer to the value.
    pub const fn as_ptr(&self) -> *const T {
        &raw const self.value.value
    }

    /// A mutable pointer to the value.
    pub fn as_mut_ptr(&mut self) -> *mut T {
        &raw mut self.value.value
    }

    /// The value, which must have been written.
    pub const unsafe fn assume_init(self) -> T {
        ManuallyDrop::into_inner(self.value)
    }

    /// A copy of the value, which must have been written; the caller
    /// makes sure only one is dropped.
    pub unsafe fn assume_init_read(&self) -> T {
        unsafe { crate::intrinsics::read(&raw const self.value.value) }
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

/// Reinterprets the bits of a value of the same size. The caller must ensure
/// that the input bits form a valid value of U. Neither value is dropped here.
#[rustc_intrinsic]
pub const unsafe fn transmute<T, U>(value: T) -> U {
    loop {}
}

/// Copies the first size_of::<Dst>() bytes, without consuming the source.
///
/// # Safety
/// The source bytes must form a valid Dst. The caller must prevent duplicate
/// use or destruction when the copy transfers ownership. Panics if Dst is larger.
pub unsafe fn transmute_copy<Src,Dst>(source:&Src)->Dst {
    assert!(size_of::<Src>()>=size_of::<Dst>(),"cannot transmute_copy if Dst is larger than Src");
    let mut result=MaybeUninit::<Dst>::uninit();
    // SAFETY: the size check bounds the read. Byte copying needs only alignment
    // one and preserves padding/initialization; result supplies Dst's alignment.
    unsafe {
        crate::ptr::copy_nonoverlapping(source as *const Src as *const u8,result.as_mut_ptr() as *mut u8,size_of::<Dst>());
        result.assume_init()
    }
}
