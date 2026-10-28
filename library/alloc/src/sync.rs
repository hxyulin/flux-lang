//! Atomic shared ownership of values and slices. The weak count includes one
//! implicit owner until the last strong owner has finished destroying T.
use core::marker::PhantomData;
use core::ops::Deref;
use core::ptr::{self, NonNull};
use core::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use core::sync::atomic::{AtomicUsize, fence};

use crate::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use crate::vec::Vec;

struct Header {
    strong: AtomicUsize,
    weak: AtomicUsize,
    layout: Layout,
}

/// Shared ownership of a value or slice. Cloning shares the allocation.
#[lang = "arc"]
pub struct Arc<T: ?Sized> {
    ptr: NonNull<Header>,
    data: NonNull<T>,
    owner: PhantomData<T>,
}

/// An allocation owner that does not keep its payload alive.
pub struct Weak<T: ?Sized> {
    ptr: Option<NonNull<Header>>,
    data: Option<NonNull<T>>,
    owner: PhantomData<T>,
}

// SAFETY: counters are atomic; accessing or transferring the shared payload
// requires both Send and Sync. Neither handle provides mutable payload access.
unsafe impl<T: ?Sized + Send + Sync> Send for Arc<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for Arc<T> {}
// SAFETY: upgrading shares T and the last strong owner may destroy T on any
// thread. The same Send + Sync requirements apply even to a weak handle.
unsafe impl<T: ?Sized + Send + Sync> Send for Weak<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for Weak<T> {}

// Check before CAS: even leaked or concurrently cloned handles cannot wrap.
fn increment(counter: &AtomicUsize) {
    let mut old = counter.load(Relaxed);
    loop {
        if old >= isize::MAX as usize {
            core::panicking::abort_now();
        }
        match counter.compare_exchange_weak(old, old + 1, Relaxed, Relaxed) {
            Ok(_) => return,
            Err(observed) => old = observed,
        }
    }
}

// get_mut briefly locks a weak count of one. A concurrent downgrade from
// another strong owner must wait for that probe, rather than overflow-abort.
fn increment_weak(counter: &AtomicUsize) {
    let mut old = counter.load(Relaxed);
    loop {
        if old == usize::MAX { old = counter.load(Relaxed); continue; }
        if old >= isize::MAX as usize { core::panicking::abort_now(); }
        match counter.compare_exchange_weak(old, old + 1, Acquire, Relaxed) {
            Ok(_) => return,
            Err(observed) => old = observed,
        }
    }
}

impl<T> Arc<T> {
    /// Moves a value into a new allocation, including for zero-sized T.
    pub fn new(value: T) -> Arc<T> {
        let (ptr, raw) = allocate(Layout::new::<T>());
        // SAFETY: aligned fresh payload storage, initialized before publication.
        unsafe {
            let data = raw as *mut T;
            ptr::write(data, value);
            Arc {
                ptr,
                data: NonNull::new_unchecked(data),
                owner: PhantomData,
            }
        }
    }
}

impl<T: Default> Default for Arc<T> {
    fn default() -> Self { Arc::new(T::default()) }
}
impl<T> Default for Arc<[T]> {
    fn default() -> Self { Arc::from(Vec::new()) }
}
impl<T: Clone> From<&[T]> for Arc<[T]> {
    fn from(values: &[T]) -> Self { Arc::from(values.to_vec()) }
}
impl<T: ?Sized> core::convert::AsRef<T> for Arc<T> {
    fn as_ref(&self) -> &T { &**self }
}

impl<T: ?Sized> Arc<T> {
    /// Creates a weak owner without extending the payload's lifetime.
    pub fn downgrade(this: &Arc<T>) -> Weak<T> {
        // SAFETY: a strong owner pins the header and its implicit weak count.
        increment_weak(unsafe { &(*this.ptr.as_ptr()).weak });
        Weak {
            ptr: Some(this.ptr),
            data: Some(this.data),
            owner: PhantomData,
        }
    }

    /// Whether two handles share an allocation.
    pub fn ptr_eq(this: &Arc<T>, other: &Arc<T>) -> bool {
        this.ptr.as_ptr() == other.ptr.as_ptr()
    }

    pub fn get_mut(this: &mut Arc<T>) -> Option<&mut T> {
        // SAFETY: this strong owner pins the initialized header.
        let header = unsafe { this.ptr.as_ref() };
        if header.weak.compare_exchange(1, usize::MAX, Acquire, Relaxed).is_err() { return None; }
        let unique = header.strong.load(Acquire) == 1;
        header.weak.store(1, Release);
        if unique {
            // SAFETY: no other strong or weak owner exists, and &mut this
            // prevents a new handle while the exclusive payload borrow lives.
            Some(unsafe { this.data.as_mut() })
        } else { None }
    }
}

struct RestoreStrong<'a> { counter: &'a AtomicUsize, armed: bool }
impl Drop for RestoreStrong<'_> {
    fn drop(&mut self) { if self.armed { self.counter.store(1, Release); } }
}

// These are the represented CloneToUninit cases: sized Clone payloads and
// slices of Clone elements. Keeping the trait private exposes one make_mut
// method without adding unstable standard-library trait names to user code.
trait ClonePayload {
    fn clone_arc(&self) -> Arc<Self>;
    unsafe fn move_arc(raw: *const Self) -> Arc<Self>;
}
impl<T: Clone> ClonePayload for T {
    fn clone_arc(&self) -> Arc<T> { Arc::new(self.clone()) }
    unsafe fn move_arc(raw: *const T) -> Arc<T> {
        let (ptr, storage) = allocate(Layout::new::<T>());
        // SAFETY: allocate completed before the move. The fresh storage is
        // aligned, and make_mut suppresses destruction of the old payload.
        unsafe {
            let data = storage as *mut T;
            ptr::write(data, ptr::read(raw));
            Arc { ptr, data: NonNull::new_unchecked(data), owner: PhantomData }
        }
    }
}
impl<T: Clone> ClonePayload for [T] {
    fn clone_arc(&self) -> Arc<[T]> { Arc::from(self.to_vec()) }
    unsafe fn move_arc(raw: *const [T]) -> Arc<[T]> {
        let len = raw.len();
        let layout = match Layout::array::<T>(len) {
            Ok(layout) => layout,
            Err(_) => crate::raw_vec::capacity_overflow(),
        };
        let (ptr, storage) = allocate(layout);
        // SAFETY: allocation completed before transferring ownership. No
        // fallible operation remains after moving the initialized elements.
        unsafe {
            let data = storage as *mut T;
            ptr::copy_nonoverlapping(raw as *const T, data, len);
            let slice = ptr::slice_from_raw_parts_mut(data, len);
            Arc { ptr, data: NonNull::new_unchecked(slice), owner: PhantomData }
        }
    }
}
#[allow(private_bounds)]
impl<T: ?Sized + ClonePayload> Arc<T> {
    pub fn make_mut(this: &mut Self) -> &mut T {
        let header = this.ptr;
        // SAFETY: this owns a strong count and the associated implicit weak count.
        let counter = unsafe { &(*header.as_ptr()).strong };
        // SAFETY: the same strong owner pins the weak counter in this header.
        let weak_count = unsafe { &(*header.as_ptr()).weak };
        if counter.compare_exchange(1, 0, Acquire, Relaxed).is_err() {
            let replacement = <T as ClonePayload>::clone_arc(&**this);
            let old = core::mem::replace(this, replacement);
            drop(old);
        } else if weak_count.load(Acquire) != 1 {
            let mut restore = RestoreStrong { counter, armed: true };
            // SAFETY: the successful CAS reserved the last strong owner.
            let replacement = unsafe { <T as ClonePayload>::move_arc(this.data.as_ptr() as *const T) };
            restore.armed = false;
            drop(restore);
            let old = core::mem::replace(this, replacement);
            // The old payload moved; release only its implicit weak owner. Any
            // surviving weak handles now observe strong == 0 and cannot upgrade.
            let weak = Weak { ptr: Some(old.ptr), data: Some(old.data), owner: PhantomData };
            core::mem::forget(old);
            drop(weak);
        } else { counter.store(1, Release); }
        // SAFETY: the retained allocation was unique, or this now holds a fresh
        // allocation after cloning/moving. No weak handle can alias its payload.
        unsafe { this.data.as_mut() }
    }
}

impl<T: ?Sized> Clone for Arc<T> {
    fn clone(&self) -> Arc<T> {
        // SAFETY: self pins the header and keeps strong above zero.
        increment(unsafe { &(*self.ptr.as_ptr()).strong });
        Arc {
            ptr: self.ptr,
            data: self.data,
            owner: PhantomData,
        }
    }
}

impl<T: ?Sized> Deref for Arc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: this strong owner keeps initialized T alive for the borrow.
        unsafe { self.data.as_ref() }
    }
}

impl<T: ?Sized + core::fmt::Debug> core::fmt::Debug for Arc<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized> Drop for Arc<T> {
    fn drop(&mut self) {
        // SAFETY: self owns one strong count; only the last owner destroys T.
        unsafe {
            if (*self.ptr.as_ptr()).strong.fetch_sub(1, Release) != 1 {
                return;
            }
            fence(Acquire);
            // Retain the implicit weak through destruction, including reentrant
            // weak drops. A guard also releases it if unwinding is added later.
            let guard: Weak<T> = Weak {
                ptr: Some(self.ptr),
                data: Some(self.data),
                owner: PhantomData,
            };
            // Dynamic drop uses payload metadata, never a reference to the header.
            ptr::drop_in_place(self.data.as_ptr());
            drop(guard);
        }
    }
}

impl<T> Weak<T> {
    /// An empty weak handle: no allocation and no invalid payload value.
    pub const fn new() -> Weak<T> {
        Weak {
            ptr: None,
            data: None,
            owner: PhantomData,
        }
    }
}

impl<T: ?Sized> Weak<T> {
    /// Acquires a strong owner only while the payload is still alive.
    pub fn upgrade(&self) -> Option<Arc<T>> {
        let ptr = self.ptr?;
        // SAFETY: this weak owner pins the initialized header, even after T dies.
        let strong = unsafe { &(*ptr.as_ptr()).strong };
        let mut old = strong.load(Relaxed);
        loop {
            if old == 0 {
                return None;
            }
            if old >= isize::MAX as usize {
                core::panicking::abort_now();
            }
            match strong.compare_exchange_weak(old, old + 1, Acquire, Relaxed) {
                Ok(_) => {
                    return Some(Arc {
                        ptr,
                        data: self.data.unwrap(),
                        owner: PhantomData,
                    });
                }
                Err(observed) => old = observed,
            }
        }
    }
}

impl<T: ?Sized> Clone for Weak<T> {
    fn clone(&self) -> Weak<T> {
        if let Some(ptr) = self.ptr {
            // SAFETY: self owns a weak count, pinning the header.
            increment_weak(unsafe { &(*ptr.as_ptr()).weak });
        }
        Weak {
            ptr: self.ptr,
            data: self.data,
            owner: PhantomData,
        }
    }
}

impl<T: ?Sized> Drop for Weak<T> {
    fn drop(&mut self) {
        if let Some(ptr) = self.ptr {
            // SAFETY: only the last weak owner frees the header. It does not
            // access dead T or run drop glue for the payload a second time.
            unsafe {
                if (*ptr.as_ptr()).weak.fetch_sub(1, Release) != 1 {
                    return;
                }
                fence(Acquire);
                let layout = (*ptr.as_ptr()).layout;
                dealloc(ptr.as_ptr() as *mut u8, layout);
            }
        }
    }
}

// Header layout is erased and stable under future array-to-slice owner unsizing.
// Keep the original allocation layout alive independently of payload metadata.
fn allocate(payload: Layout) -> (NonNull<Header>, *mut u8) {
    let header = Layout::new::<Header>();
    let align = if header.align() > payload.align() {
        header.align()
    } else {
        payload.align()
    };
    let padded = match header.size().checked_add(payload.align() - 1) {
        Some(size) => size,
        None => crate::raw_vec::capacity_overflow(),
    };
    let offset = padded & !(payload.align() - 1);
    let size = match offset.checked_add(payload.size()) {
        Some(size) => size,
        None => crate::raw_vec::capacity_overflow(),
    };
    let layout = match Layout::from_size_align(size, align) {
        Ok(layout) => layout,
        Err(_) => crate::raw_vec::capacity_overflow(),
    };
    // SAFETY: the nonempty header guarantees a nonzero allocation size.
    unsafe {
        let base = alloc(layout);
        if base.is_null() {
            handle_alloc_error(layout);
        }
        let raw = base as *mut Header;
        ptr::write(
            raw,
            Header {
                strong: AtomicUsize::new(1),
                weak: AtomicUsize::new(1),
                layout,
            },
        );
        (NonNull::new_unchecked(raw), base.add(offset))
    }
}

impl<T> From<Vec<T>> for Arc<[T]> {
    fn from(mut values: Vec<T>) -> Arc<[T]> {
        let len = values.len();
        let layout = match Layout::array::<T>(len) {
            Ok(layout) => layout,
            Err(_) => crate::raw_vec::capacity_overflow(),
        };
        let (ptr, raw) = allocate(layout);
        // SAFETY: separate aligned storage for len elements; moving bytes then
        // clearing the source length transfers ownership without cloning. The
        // slice is formed only after all elements are initialized (also ZSTs).
        unsafe {
            let data = raw as *mut T;
            ptr::copy_nonoverlapping(values.as_ptr(), data, len);
            values.set_len(0);
            let slice = core::slice::from_raw_parts_mut(data, len);
            Arc {
                ptr,
                data: NonNull::new_unchecked(slice as *mut [T]),
                owner: PhantomData,
            }
        }
    }
}

impl<T> FromIterator<T> for Arc<[T]> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Arc<[T]> {
        let values: Vec<T> = iter.into_iter().collect();
        values.into()
    }
}

impl<T: ?Sized + core::cmp::PartialEq> core::cmp::PartialEq for Arc<T> {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl<T: ?Sized + core::cmp::Eq> core::cmp::Eq for Arc<T> {}

impl<T: ?Sized + core::hash::Hash> core::hash::Hash for Arc<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}

impl<T> From<crate::boxed::Box<T>> for Arc<T> {
    fn from(value: crate::boxed::Box<T>) -> Self {
        // Allocate before moving, so allocation failure cannot consume T.
        let (ptr, raw) = allocate(Layout::new::<T>());
        let source = crate::boxed::Box::into_raw(value);
        // SAFETY: source is an initialized owned Box allocation; raw is a
        // distinct aligned destination. Transfer T once, then free only the
        // source allocation, never its moved payload.
        unsafe {
            let data = raw as *mut T;
            ptr::write(data, ptr::read(source));
            let layout = Layout::new::<T>();
            if layout.size() != 0 { dealloc(source as *mut u8, layout); }
            Arc { ptr, data: NonNull::new_unchecked(data), owner: PhantomData }
        }
    }
}
impl<T> From<crate::boxed::Box<[T]>> for Arc<[T]> {
    fn from(value: crate::boxed::Box<[T]>) -> Self { Arc::from(value.into_vec()) }
}
impl From<crate::boxed::Box<str>> for Arc<str> {
    fn from(value: crate::boxed::Box<str>) -> Self {
        let len = value.len();
        let layout = Layout::array::<u8>(len).unwrap();
        let (ptr, raw) = allocate(layout);
        let source = crate::boxed::Box::into_raw(value);
        // SAFETY: both allocations hold len bytes, raw is distinct storage,
        // and copying UTF-8 preserves validity. Transfer ownership before
        // freeing the original allocation using its exact layout.
        unsafe {
            ptr::copy_nonoverlapping(source as *const u8, raw, len);
            if len != 0 { dealloc(source as *mut u8, layout); }
            let bytes = core::slice::from_raw_parts_mut(raw, len);
            Arc { ptr, data: NonNull::new_unchecked(bytes as *mut [u8] as *mut str), owner: PhantomData }
        }
    }
}

// Moving the owner does not pin or move the separately allocated payload.
impl<T: ?Sized> core::marker::Unpin for Arc<T> {}
impl<T: ?Sized> core::marker::Unpin for Weak<T> {}
