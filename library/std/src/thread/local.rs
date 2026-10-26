//! POSIX thread-local values, initialized lazily and destroyed on thread exit.

use crate::alloc_crate::boxed::Box;
use core::fmt;
use core::ops::{Drop, FnOnce};
use core::ptr;
use super::super::sync::OnceLock;

#[cfg(target_os = "macos")]
type Key = usize;
#[cfg(not(target_os = "macos"))]
type Key = u32;

unsafe extern "C" {
    fn pthread_key_create(key: *mut Key, destructor: unsafe extern "C" fn(*mut u8)) -> i32;
    fn pthread_getspecific(key: Key) -> *mut u8;
    fn pthread_setspecific(key: Key, value: *const u8) -> i32;
}

// Apple does not run pthread key destructors when the main thread returns.
// It also forbids registering a new TLV callback during TLS destruction, so
// register one callback per thread and keep all payloads in our own list.
#[cfg(target_os = "macos")]
mod apple {
    use super::Key;
    use crate::alloc_crate::boxed::Box;
    use crate::alloc_crate::vec::Vec;
    use crate::sync::OnceLock;
    use core::ptr;

    unsafe extern "C" {
        fn _tlv_atexit(destructor: unsafe extern "C" fn(*mut u8), argument: *mut u8);
        fn pthread_main_np() -> i32;
    }

    struct Entry {
        key: Key,
        destructor: unsafe extern "C" fn(*mut u8),
    }

    struct Registry {
        entries: Vec<Entry>,
        dropless: Vec<Entry>,
        running: bool,
    }

    static KEY: OnceLock<Key> = OnceLock::new();

    // The registry key is distinct from payload keys. Its completed marker
    // remains installed during subsequent POSIX destructor passes.
    fn completed() -> *mut u8 { 1usize as *mut u8 }

    fn key() -> Key {
        *KEY.get_or_init(|| {
            let mut key: Key = 0;
            // SAFETY: the key outlives every registered per-thread registry.
            assert_eq!(unsafe { super::pthread_key_create(&mut key, fallback) }, 0);
            key
        })
    }

    /// Called before running user initialization. False means this thread's
    /// complete destructor list already ran; registering another TLV callback
    /// would be undefined behavior.
    pub(super) fn enable() -> bool {
        let key = key();
        // SAFETY: this key is initialized once and belongs to the calling thread.
        let registry = unsafe { super::pthread_getspecific(key) };
        if registry == completed() { return false; }
        if registry.is_null() {
            let registry = Box::into_raw(Box::new(Registry { entries: Vec::new(), dropless: Vec::new(), running: false }));
            // SAFETY: publish before registration or user code can reenter.
            assert_eq!(unsafe { super::pthread_setspecific(key, registry as *mut u8) }, 0);
            // SAFETY: a null registry is possible only before this thread's
            // first registration. Draining retains the pointer, then replaces
            // it with completed(), so cleanup can never register again.
            unsafe { _tlv_atexit(cleanup, ptr::null_mut()); }
        }
        true
    }

    pub(super) fn register(key: Key, destructor: unsafe extern "C" fn(*mut u8), needs_drop: bool) {
        // SAFETY: enable ran before this key's initializer. The registry stays
        // allocated through every payload destructor and is thread-confined.
        let registry = unsafe { super::pthread_getspecific(self::key()) } as *mut Registry;
        // SAFETY: no borrow of entries survives a call to a payload destructor.
        unsafe {
            if needs_drop { (*registry).entries.push(Entry { key, destructor }); }
            else { (*registry).dropless.push(Entry { key, destructor }); }
        }
    }

    unsafe extern "C" fn cleanup(_: *mut u8) {
        let key = key();
        // SAFETY: inspect the key rather than a callback argument that a prior
        // POSIX fallback might already have freed.
        let registry = unsafe { super::pthread_getspecific(key) };
        if registry.is_null() || registry == completed() { return; }
        // SAFETY: this is the current thread's live, published registry.
        unsafe { drain(registry as *mut Registry); }
    }

    unsafe extern "C" fn fallback(pointer: *mut u8) {
        let key = key();
        // POSIX clears its key before invoking a destructor. Reinstall it so
        // payload destructors can find the current registry or completed state.
        // SAFETY: pointer is the value POSIX just removed from this thread's key.
        assert_eq!(unsafe { super::pthread_setspecific(key, pointer) }, 0);
        if pointer == completed() { return; }
        // SAFETY: a non-marker key value is a live Box<Registry>.
        unsafe { drain(pointer as *mut Registry); }
    }

    unsafe fn drain(registry: *mut Registry) {
        // SAFETY: only the current thread accesses this allocation. The flag
        // prevents reentrant cleanup, and no reference survives destructor calls.
        if unsafe { (*registry).running } { return; }
        unsafe { (*registry).running = true; }
        loop {
            // SAFETY: pop ends its mutable access before invoking user Drop.
            let next = unsafe { (*registry).entries.pop() };
            match next {
                Some(entry) => {
                    // A POSIX callback may already have destroyed this key.
                    // Consult its current value rather than retaining payload
                    // addresses that could have been freed by that callback.
                    // SAFETY: payload keys remain allocated for process lifetime.
                    let pointer = unsafe { super::pthread_getspecific(entry.key) };
                    if !pointer.is_null() && (pointer as usize) & 3 == 0 {
                        // SAFETY: the entry records this key's concrete destructor.
                        unsafe { (entry.destructor)(pointer); }
                    }
                }
                None => break,
            }
        }
        // Values without drop glue must remain accessible to every payload
        // destructor, regardless of their initialization order. Reclaim their
        // backing allocations only after the reentrant destructor list empties.
        loop {
            // SAFETY: dropless values run no user destructor on reclamation.
            let next = unsafe { (*registry).dropless.pop() };
            match next {
                Some(entry) => {
                    // SAFETY: inspect this thread's still-allocated payload key.
                    let pointer = unsafe { super::pthread_getspecific(entry.key) };
                    if !pointer.is_null() && (pointer as usize) & 3 == 0 {
                        // SAFETY: register paired this key with its concrete slot destructor.
                        unsafe { (entry.destructor)(pointer); }
                    }
                }
                None => break,
            }
        }
        // SAFETY: no payload remains. Publish completion before deallocating
        // the list; a subsequent callback must never read the freed registry.
        assert_eq!(unsafe { super::pthread_setspecific(key(), completed()) }, 0);
        unsafe { drop(Box::from_raw(registry)); }
        // The entry point flushes before Apple runs main-thread TLS callbacks.
        // Use the existing compiler runtime flush after those callbacks too,
        // so a destructor's final partial line reaches stdout. Worker threads
        // retain the normal process-wide stdout buffering behavior.
        // SAFETY: pthread_main_np only queries the calling thread's identity.
        if unsafe { pthread_main_np() } != 0 { core::io::flush_stdout(); }
    }
}

/// A value belonging to the calling thread. Shared handles never share payloads.
pub struct LocalKey<T: 'static> {
    key: OnceLock<Key>,
    init: fn() -> T,
}

/// The current thread is destroying this thread-local value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccessError;

impl fmt::Display for AccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("already destroyed")
    }
}
impl core::error::Error for AccessError {}

// Real allocations are aligned to at least four bytes because Slot contains Key.
// The low two address bits identify initializer/destructor markers instead.
fn marker(key: Key, state: usize) -> *mut u8 {
    (((key as usize) << 2) | state) as *mut u8
}

struct Slot<T> {
    key: Key,
    value: T,
}

unsafe extern "C" fn destroy<T>(pointer: *mut u8) {
    let bits = pointer as usize;
    if bits & 3 != 0 {
        // POSIX clears a key before its destructor. Reinstall the destroyed
        // marker on every destructor pass so another TLS destructor cannot
        // initialize this key again. The marker owns no allocation.
        let key = (bits >> 2) as Key;
        // SAFETY: the marker encodes the still-live key that invoked this callback.
        unsafe { pthread_setspecific(key, marker(key, 3)); }
        return;
    }
    // SAFETY: pthread calls this once with the Box<Slot<T>> published for its key.
    let slot = unsafe { Box::from_raw(pointer as *mut Slot<T>) };
    let key = slot.key;
    // SAFETY: this thread owns this key's payload and marks it before running T's Drop.
    assert_eq!(unsafe { pthread_setspecific(key, marker(key, 3)) }, 0);
    drop(slot);
}

// POSIX may visit a dropless payload key before the registry fallback.
// Retain its allocation for the remaining payload destructors in that case.
unsafe extern "C" fn posix_destroy<T>(pointer: *mut u8) {
    #[cfg(target_os = "macos")]
    if (pointer as usize) & 3 == 0 && !core::mem::needs_drop::<T>() {
        // SAFETY: a non-marker value is this key's live Slot<T>; POSIX just
        // removed it, and the registry still owns its eventual reclamation.
        let key = unsafe { (*(pointer as *const Slot<T>)).key };
        assert_eq!(unsafe { pthread_setspecific(key, pointer) }, 0);
        return;
    }
    // SAFETY: all remaining values use the ordinary payload/marker destructor.
    unsafe { destroy::<T>(pointer); }
}

struct Initializing {
    key: Key,
    armed: bool,
}
impl Drop for Initializing {
    fn drop(&mut self) {
        if self.armed {
            // SAFETY: initialization did not publish an allocation. A retry is allowed.
            unsafe { pthread_setspecific(self.key, ptr::null()); }
        }
    }
}

impl<T: 'static> LocalKey<T> {
    /// Implementation entry point used by the thread_local macro.
    pub const fn __new(init: fn() -> T) -> LocalKey<T> {
        LocalKey { key: OnceLock::new(), init }
    }

    pub fn with<F: FnOnce(&T) -> R, R>(&'static self, f: F) -> R {
        self.try_with(f).expect("cannot access a Thread Local Storage value during or after destruction")
    }

    pub fn try_with<F: FnOnce(&T) -> R, R>(&'static self, f: F) -> Result<R, AccessError> {
        let (key, mut pointer) = self.storage()?;
        if pointer.is_null() {
            pointer = self.initialize(key, self.init)?;
        }
        // SAFETY: storage rejects destroyed/initializing keys, and initialization
        // publishes a live Slot<T> owned by this thread before returning.
        Ok(f(unsafe { &(*(pointer as *const Slot<T>)).value }))
    }

    fn storage(&'static self) -> Result<(Key, *mut u8), AccessError> {
        let key = *self.key.get_or_init(|| {
            let mut key: Key = 0;
            // SAFETY: key points to writable storage with the target's pthread_key_t size.
            assert_eq!(unsafe { pthread_key_create(&mut key, posix_destroy::<T>) }, 0);
            assert!((key as usize) <= usize::MAX >> 2, "thread-local key is too large");
            key
        });
        // SAFETY: key was published only after successful native initialization.
        let pointer = unsafe { pthread_getspecific(key) };
        let state = (pointer as usize) & 3;
        if state == 1 {
            panic!("cannot recursively initialize thread-local storage");
        }
        if state != 0 {
            return Err(AccessError);
        }
        Ok((key, pointer))
    }

    fn initialize<I: FnOnce() -> T>(&'static self, key: Key, init: I) -> Result<*mut u8, AccessError> {
        #[cfg(target_os = "macos")]
        if !apple::enable() { return Err(AccessError); }
        // SAFETY: storage found an empty key on this thread. Mark it before
        // calling user code, so recursive access observes initialization.
        assert_eq!(unsafe { pthread_setspecific(key, marker(key, 1)) }, 0);
        let mut guard = Initializing { key, armed: true };
        let slot = Box::new(Slot { key, value: init() });
        let pointer = Box::into_raw(slot) as *mut u8;
        // SAFETY: the key owns this allocation until its registered destructor runs.
        assert_eq!(unsafe { pthread_setspecific(key, pointer) }, 0);
        #[cfg(target_os = "macos")]
        apple::register(key, destroy::<T>, core::mem::needs_drop::<T>());
        guard.armed = false;
        Ok(pointer)
    }
}

impl<T: 'static> LocalKey<core::cell::Cell<T>> {
    /// Setting an uninitialized key supplies its initial value directly;
    /// the thread_local initializer is deliberately not run.
    pub fn set(&'static self, value: T) {
        let (key, pointer) = self.storage()
            .expect("cannot access a Thread Local Storage value during or after destruction");
        if pointer.is_null() {
            self.initialize(key, || core::cell::Cell::new(value))
                .expect("cannot access a Thread Local Storage value during or after destruction");
        } else {
            // SAFETY: storage returned this thread's live Slot<Cell<T>>.
            unsafe { &(*(pointer as *const Slot<core::cell::Cell<T>>)).value }.set(value);
        }
    }
}

impl<T: Copy + 'static> LocalKey<core::cell::Cell<T>> {
    pub fn get(&'static self) -> T { self.with(|cell| cell.get()) }
}
