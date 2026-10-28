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
