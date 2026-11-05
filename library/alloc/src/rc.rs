//! Non-atomic shared ownership with separately typed payload metadata.
use crate::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use crate::vec::Vec;
use core::cell::Cell;
use core::marker::PhantomData;
use core::ops::Deref;
use core::ptr::{self, NonNull};

struct Header {
    strong: Cell<usize>,
    layout: Layout,
}

/// Shared ownership confined to one thread. NonNull keeps Rc !Send and !Sync.
// `rc_ownership` compiles this file with rustc as a test, where `test` is set
// and `lang` items are unstable.
#[cfg_attr(not(test), lang = "rc")]
pub struct Rc<T: ?Sized> {
    ptr: NonNull<Header>,
    data: NonNull<T>,
    owner: PhantomData<T>,
}

impl<T> Rc<T> {
    pub fn new(value: T) -> Rc<T> {
        let (ptr, raw) = allocate(Layout::new::<T>());
        // SAFETY: fresh aligned storage is initialized before publication.
        unsafe {
            let data = raw as *mut T;
            ptr::write(data, value);
            Rc {
                ptr,
                data: NonNull::new_unchecked(data),
                owner: PhantomData,
            }
        }
    }

    pub fn try_unwrap(this: Rc<T>) -> Result<T, Rc<T>> {
        if Rc::strong_count(&this) != 1 {
            return Err(this);
        }
        let header = this.ptr;
        let data = this.data;
        core::mem::forget(this);
        // SAFETY: the only owner was consumed without running Drop. Reading
        // transfers T before freeing its allocation, so T is not destroyed here.
        unsafe {
            let value = ptr::read(data.as_ptr());
            let layout = (*header.as_ptr()).layout;
            dealloc(header.as_ptr() as *mut u8, layout);
            Ok(value)
        }
    }
}

impl<T: ?Sized> Rc<T> {
    pub fn strong_count(this: &Rc<T>) -> usize {
        // SAFETY: this handle keeps the header alive throughout the borrow.
        unsafe { (*this.ptr.as_ptr()).strong.get() }
    }

    pub fn ptr_eq(this: &Rc<T>, other: &Rc<T>) -> bool {
        this.ptr.as_ptr() == other.ptr.as_ptr()
    }

    pub fn get_mut(this: &mut Rc<T>) -> Option<&mut T> {
        if Rc::strong_count(this) != 1 {
            return None;
        }
        // SAFETY: the only strong owner is exclusively borrowed. There are no
        // weak owners, so no other handle can acquire or access the payload.
        Some(unsafe { this.data.as_mut() })
    }
}

impl<T: Clone> Rc<T> {
    pub fn make_mut(this: &mut Rc<T>) -> &mut T {
        if Rc::strong_count(this) != 1 {
            // Complete cloning/allocation before releasing this owner; a Clone
            // panic leaves the original handle and reference count intact.
            let replacement = Rc::new((**this).clone());
            *this = replacement;
        }
        Rc::get_mut(this).unwrap()
    }

    pub fn unwrap_or_clone(this: Rc<T>) -> T {
        match Rc::try_unwrap(this) {
            Ok(value) => value,
            Err(shared) => (*shared).clone(),
        }
    }
}

impl<T: ?Sized> Clone for Rc<T> {
    fn clone(&self) -> Rc<T> {
        // SAFETY: self pins the header; this non-atomic owner cannot cross threads.
        let count = unsafe { &(*self.ptr.as_ptr()).strong };
        let strong = count.get();
        if strong >= isize::MAX as usize {
            // SAFETY: self keeps the allocation/header layout initialized.
            handle_alloc_error(unsafe { (*self.ptr.as_ptr()).layout });
        }
        count.set(strong + 1);
        Rc {
            ptr: self.ptr,
            data: self.data,
            owner: PhantomData,
        }
    }
}

impl<T: ?Sized> Deref for Rc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: this owner keeps the initialized payload and its metadata live.
        unsafe { self.data.as_ref() }
    }
}
impl<T: ?Sized> AsRef<T> for Rc<T> {
    fn as_ref(&self) -> &T {
        self
    }
}

struct Release {
    ptr: NonNull<Header>,
}
impl Drop for Release {
    fn drop(&mut self) {
        // SAFETY: the final strong owner transfers the allocation to this
        // guard, which releases it even when the payload destructor unwinds.
        unsafe {
            let layout = (*self.ptr.as_ptr()).layout;
            dealloc(self.ptr.as_ptr() as *mut u8, layout);
        }
    }
}
impl<T: ?Sized> Drop for Rc<T> {
    fn drop(&mut self) {
        // SAFETY: self owns one count. Only its final owner destroys T; the
        // allocation stays live until the release guard is dropped afterward.
        unsafe {
            let count = &(*self.ptr.as_ptr()).strong;
            let strong = count.get();
            count.set(strong - 1);
            if strong == 1 {
                let _release = Release { ptr: self.ptr };
                ptr::drop_in_place(self.data.as_ptr());
            }
        }
    }
}

fn allocate(payload: Layout) -> (NonNull<Header>, *mut u8) {
    let header = Layout::new::<Header>();
    let align = header.align().max(payload.align());
    let padded = match header.size().checked_add(payload.align() - 1) {
        Some(size) => size,
        None => handle_alloc_error(payload),
    };
    let offset = padded & !(payload.align() - 1);
    let size = match offset.checked_add(payload.size()) {
        Some(size) => size,
        None => handle_alloc_error(payload),
    };
    let layout = match Layout::from_size_align(size, align) {
        Ok(layout) => layout,
        Err(_) => handle_alloc_error(payload),
    };
    // SAFETY: a nonempty header ensures nonzero allocation size. The checked
    // offset aligns the payload and the layout covers both header and payload.
    unsafe {
        let base = alloc(layout);
        if base.is_null() {
            handle_alloc_error(layout);
        }
        let ptr = base as *mut Header;
        ptr::write(
            ptr,
            Header {
                strong: Cell::new(1),
                layout,
            },
        );
        (NonNull::new_unchecked(ptr), base.add(offset))
    }
}

impl<T> From<Vec<T>> for Rc<[T]> {
    fn from(mut values: Vec<T>) -> Rc<[T]> {
        let len = values.len();
        let layout = match Layout::array::<T>(len) {
            Ok(layout) => layout,
            Err(_) => handle_alloc_error(Layout::new::<Header>()),
        };
        let (ptr, raw) = allocate(layout);
        // SAFETY: the destination is separate aligned storage for len values.
        // Clearing the source length transfers ownership, including ZSTs.
        unsafe {
            let data = raw as *mut T;
            ptr::copy_nonoverlapping(values.as_ptr(), data, len);
            values.set_len(0);
            let slice = core::slice::from_raw_parts_mut(data, len);
            Rc {
                ptr,
                data: NonNull::new_unchecked(slice as *mut [T]),
                owner: PhantomData,
            }
        }
    }
}
impl<T> Default for Rc<[T]> {
    fn default() -> Self {
        Vec::new().into()
    }
}
impl<T: Clone> From<&[T]> for Rc<[T]> {
    fn from(values: &[T]) -> Self {
        values.to_vec().into()
    }
}

// Moving the owner does not pin or move the separately allocated payload.
impl<T: ?Sized> core::marker::Unpin for Rc<T> {}

impl<T: ?Sized + core::panic::RefUnwindSafe> core::panic::UnwindSafe for Rc<T> {}
impl<T: ?Sized + core::panic::RefUnwindSafe> core::panic::RefUnwindSafe for Rc<T> {}

#[cfg(test)]
mod tests {
    use super::Rc;
    #[test]
    fn count_overflow_aborts() {
        if std::env::var_os("FLUX_RC_OVERFLOW_CHILD").is_some() {
            let owner = Rc::new(7u32);
            // SAFETY: the handle keeps this allocation live. Only its Cell
            // counter is changed, and this child must abort before destruction.
            unsafe {
                (*owner.ptr.as_ptr()).strong.set(isize::MAX as usize);
            }
            let _ = owner.clone();
            panic!("overflow clone returned");
        }
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "rc::tests::count_overflow_aborts"])
            .env("FLUX_RC_OVERFLOW_CHILD", "1")
            .output()
            .unwrap();
        assert!(
            result.status.code().is_none(),
            "overflow must abort: {:?}",
            result.status
        );
    }
}

// The payload starts at the header size rounded up to its alignment, as
// allocate places it; the value's own metadata gives that alignment.
unsafe fn header_of<T: ?Sized>(data: *const T) -> NonNull<Header> {
    // SAFETY: the caller passes a payload pointer that allocate produced.
    unsafe {
        let align = Layout::for_value_raw(data).align();
        let offset = (core::mem::size_of::<Header>() + align - 1) & !(align - 1);
        NonNull::new_unchecked((data as *const u8).sub(offset) as *mut Header)
    }
}

impl<T: ?Sized> Rc<T> {
    /// Returns the payload pointer without changing the strong count.
    pub fn as_ptr(this: &Self) -> *const T { this.data.as_ptr() }
    /// Consumes the handle, keeping its strong count, and returns the payload pointer.
    pub fn into_raw(this: Self) -> *const T {
        let data = this.data.as_ptr() as *const T;
        core::mem::forget(this);
        data
    }
    /// Rebuilds a handle from `into_raw`'s pointer, taking over its strong count.
    ///
    /// # Safety
    /// `ptr` must come from `Rc::into_raw` for the same payload type, and its
    /// strong count must not have been reclaimed already.
    pub unsafe fn from_raw(ptr: *const T) -> Self {
        // SAFETY: forwarded from the caller's contract.
        unsafe { Rc { ptr: header_of(ptr), data: NonNull::new_unchecked(ptr as *mut T), owner: PhantomData } }
    }
    /// # Safety
    /// `ptr` must come from `Rc::into_raw` and its allocation must be alive.
    pub unsafe fn increment_strong_count(ptr: *const T) {
        // SAFETY: forwarded from the caller's contract; forgetting the
        // clone's source keeps the borrowed count unchanged.
        unsafe {
            let this = core::mem::ManuallyDrop::new(Rc::from_raw(ptr));
            core::mem::forget(Rc::clone(&this));
        }
    }
    /// # Safety
    /// `ptr` must come from `Rc::into_raw`, and the call releases one strong count.
    pub unsafe fn decrement_strong_count(ptr: *const T) {
        // SAFETY: forwarded from the caller's contract.
        drop(unsafe { Rc::from_raw(ptr) });
    }
}

fn bytes_rc(bytes: &[u8]) -> Rc<[u8]> { Rc::from(bytes) }
impl From<&str> for Rc<str> {
    fn from(value: &str) -> Rc<str> {
        let bytes = bytes_rc(value.as_bytes());
        // SAFETY: the bytes are valid UTF-8, and str has [u8]'s layout and metadata.
        unsafe { Rc::from_raw(Rc::into_raw(bytes) as *const str) }
    }
}
impl From<&mut str> for Rc<str> {
    fn from(value: &mut str) -> Rc<str> { Rc::from(&*value) }
}
impl From<crate::string::String> for Rc<str> {
    fn from(value: crate::string::String) -> Rc<str> { Rc::from(value.as_str()) }
}
impl From<&crate::string::String> for Rc<str> {
    fn from(value: &crate::string::String) -> Rc<str> { Rc::from(value.as_str()) }
}
impl From<crate::boxed::Box<str>> for Rc<str> {
    fn from(value: crate::boxed::Box<str>) -> Rc<str> { Rc::from(&*value) }
}
impl<T> From<crate::boxed::Box<[T]>> for Rc<[T]> {
    fn from(value: crate::boxed::Box<[T]>) -> Rc<[T]> { Rc::from(value.into_vec()) }
}
impl<T> From<crate::boxed::Box<T>> for Rc<T> {
    fn from(value: crate::boxed::Box<T>) -> Rc<T> { Rc::new(*value) }
}
impl<'a, B: crate::borrow::ToOwned + ?Sized> From<crate::borrow::Cow<'a, B>> for Rc<B>
where Rc<B>: From<&'a B> + From<B::Owned> {
    fn from(value: crate::borrow::Cow<'a, B>) -> Rc<B> {
        match value {
            crate::borrow::Cow::Borrowed(value) => Rc::from(value),
            crate::borrow::Cow::Owned(value) => Rc::from(value),
        }
    }
}
impl From<Rc<str>> for Rc<[u8]> {
    fn from(value: Rc<str>) -> Rc<[u8]> {
        // SAFETY: str has [u8]'s layout and metadata, and any bytes are valid.
        unsafe { Rc::from_raw(Rc::into_raw(value) as *const [u8]) }
    }
}
impl<T> From<T> for Rc<T> {
    fn from(value: T) -> Rc<T> { Rc::new(value) }
}
impl<T: Clone> From<&mut [T]> for Rc<[T]> {
    fn from(values: &mut [T]) -> Self { Rc::from(values.to_vec()) }
}
impl<T, const N: usize> From<[T; N]> for Rc<[T]> {
    fn from(values: [T; N]) -> Self { Rc::from(Vec::from(values)) }
}
impl<T> FromIterator<T> for Rc<[T]> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Rc<[T]> {
        let values: Vec<T> = iter.into_iter().collect();
        values.into()
    }
}
impl<T: Default> Default for Rc<T> {
    fn default() -> Self { Rc::new(T::default()) }
}
impl Default for Rc<str> {
    fn default() -> Self { Rc::from("") }
}
impl<T: ?Sized + core::fmt::Debug> core::fmt::Debug for Rc<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { core::fmt::Debug::fmt(&**self, f) }
}
impl<T: ?Sized + core::fmt::Display> core::fmt::Display for Rc<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { core::fmt::Display::fmt(&**self, f) }
}
impl<T: ?Sized> core::fmt::Pointer for Rc<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Pointer::fmt(&(self.data.as_ptr() as *const T), f)
    }
}
impl<T: ?Sized + PartialEq> PartialEq for Rc<T> {
    fn eq(&self, other: &Self) -> bool { **self == **other }
}
impl<T: ?Sized + Eq> Eq for Rc<T> {}
impl<T: ?Sized + PartialOrd> PartialOrd for Rc<T> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> { (**self).partial_cmp(&**other) }
}
impl<T: ?Sized + Ord> Ord for Rc<T> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering { (**self).cmp(&**other) }
}
impl<T: ?Sized + core::hash::Hash> core::hash::Hash for Rc<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) { (**self).hash(state) }
}
impl<T: ?Sized> core::borrow::Borrow<T> for Rc<T> {
    fn borrow(&self) -> &T { &**self }
}
impl<T: ?Sized + core::error::Error> core::error::Error for Rc<T> {
    #[allow(deprecated)]
    fn description(&self) -> &str { core::error::Error::description(&**self) }
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> { core::error::Error::source(&**self) }
}
