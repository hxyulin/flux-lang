//! Native workers whose borrowed environments stay alive until every join.

use crate::alloc_crate::boxed::Box;
use crate::alloc_crate::sync::Arc;
use crate::alloc_crate::vec::Vec;
use crate::sync::Mutex;
use core::cell::UnsafeCell;
use core::marker::{PhantomData, Send, Sync};
use core::ops::{Drop, FnOnce};
use core::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "macos")]
type NativeThread = *mut u8;
#[cfg(not(target_os = "macos"))]
type NativeThread = usize;

unsafe extern "C" {
    fn pthread_create(
        thread: *mut NativeThread,
        attr: *const u8,
        callback: extern "C" fn(*mut u8) -> *mut u8,
        arg: *mut u8,
    ) -> i32;
    fn pthread_join(thread: NativeThread, result: *mut *mut u8) -> i32;
    fn pthread_detach(thread: NativeThread) -> i32;
    fn pthread_attr_init(attr: *mut u64) -> i32;
    fn pthread_attr_destroy(attr: *mut u64) -> i32;
    fn pthread_attr_setstacksize(attr: *mut u64, size: usize) -> i32;
    fn abort() -> !;
}

struct JoinState {
    thread: Mutex<Option<NativeThread>>,
    panicked: AtomicBool,
}

// SAFETY: native identifiers are accessed only under the mutex and joined once.
unsafe impl Send for JoinState {}
// SAFETY: the mutex serializes even concurrent calls to join.
unsafe impl Sync for JoinState {}

impl Drop for JoinState {
    fn drop(&mut self) {
        if let Some(id) = self.thread.get_mut().unwrap().take() {
            // SAFETY: final ownership proves no join can still consume this
            // identifier. A detached pthread reclaims its resources on exit.
            if unsafe { pthread_detach(id) } != 0 {
                // SAFETY: an invalid identifier indicates a broken lifecycle;
                // continuing could leak or reuse an unowned native thread.
                unsafe { abort() }
            }
        }
    }
}

impl JoinState {
    fn detach(&self) {
        if let Some(id) = self.thread.lock().unwrap().take() {
            // SAFETY: taking the identifier excludes every future join or
            // detach. pthread_detach does not wait for the worker to finish.
            if unsafe { pthread_detach(id) } != 0 {
                // SAFETY: an invalid identifier indicates a broken lifecycle;
                // continuing could leak or reuse an unowned native thread.
                unsafe { abort() }
            }
        }
    }
    fn join(&self) {
        let mut thread = self.thread.lock().unwrap();
        if let Some(id) = thread.take() {
            // SAFETY: a successful create installed this identifier; taking it
            // under the lock gives this caller sole responsibility for joining.
            if unsafe { pthread_join(id, core::ptr::null_mut()) } != 0 {
                // SAFETY: returning or unwinding without a completed join
                // could release borrowed storage still used by the worker.
                unsafe { abort() }
            }
        }
    }
}

pub struct Scope<'scope, 'env: 'scope> {
    threads: Arc<Threads>,
    scope: PhantomData<&'scope mut &'scope ()>,
    env: PhantomData<&'env mut &'env ()>,
}

struct Threads {
    registry: Mutex<Vec<Arc<JoinState>>>,
}

impl Drop for Threads {
    fn drop(&mut self) {
        // Workers may spawn further workers. Never hold the registry lock
        // while joining, and keep draining until those workers also finish.
        let mut unhandled = false;
        loop {
            let next = self.registry.lock().unwrap().pop();
            match next {
                Some(thread) => {
                    thread.join();
                    unhandled |= thread.panicked.load(Ordering::Acquire);
                }
                None => break,
            }
        }
        // Join every worker before reporting a failure. A parent panic already
        // in flight keeps its original payload rather than causing a double panic.
        if unhandled && !super::panicking() { panic!("a scoped thread panicked"); }
    }
}

pub fn scope<'env, F, T>(f: F) -> T
where
    F: for<'scope> FnOnce(&'scope Scope<'scope, 'env>) -> T,
{
    let scope = Scope {
        threads: Arc::new(Threads {
            registry: Mutex::new(Vec::new()),
        }),
        scope: PhantomData,
        env: PhantomData,
    };
    f(&scope)
}

struct Packet<T> {
    value: UnsafeCell<Option<super::Result<T>>>,
    finished: AtomicBool,
    join: Arc<JoinState>,
}

// SAFETY: the worker alone writes the value; the handle reads only after join.
unsafe impl<T: Send> Send for Packet<T> {}
// SAFETY: joining synchronizes publication and the handle is consumed on read.
unsafe impl<T: Send> Sync for Packet<T> {}

struct Job<F, T> {
    capture: Option<Arc<Mutex<Vec<u8>>>>,
    function: F,
    packet: Arc<Packet<T>>,
    thread: super::Thread,
}

extern "C" fn run<F: FnOnce() -> T, T>(argument: *mut u8) -> *mut u8 {
    // SAFETY: spawn transferred precisely one Box<Job<F, T>> to this callback.
    let job = unsafe { *Box::from_raw(argument as *mut Job<F, T>) };
    super::set_current(job.thread);
    if job.capture.is_some() {crate::io::__set_output_capture(job.capture);}
    // No exception may cross pthread's C entry point. The catch consumes the
    // function once and restores this worker's panic state before publication.
    let value = crate::panic::catch_unwind(core::panic::AssertUnwindSafe(job.function));
    job.packet.join.panicked.store(value.is_err(), Ordering::Release);
    // SAFETY: the worker has sole access until pthread_join completes.
    unsafe {
        *job.packet.value.get() = Some(value);
    }
    job.packet.finished.store(true, Ordering::Release);
    core::ptr::null_mut()
}

pub struct ScopedJoinHandle<'scope, T> {
    packet: Arc<Packet<T>>,
    thread: super::Thread,
    scope: PhantomData<&'scope mut &'scope ()>,
}

/// An owned native worker. Dropping its handle detaches without waiting.
pub struct JoinHandle<T> {
    packet: Arc<Packet<T>>,
    thread: super::Thread,
}

// pthread_attr_t is 64 bytes on Darwin and 56 on glibc.
struct Attributes { words: [u64; 8] }
// std's default stack for spawned threads, without RUST_MIN_STACK.
const DEFAULT_STACK: usize = 2 * 1024 * 1024;
// PTHREAD_STACK_MIN, and the page size a rejected size is rounded up to.
#[cfg(target_os = "macos")]
const STACK_MIN: usize = 16384;
#[cfg(target_os = "linux")]
const STACK_MIN: usize = 16384;
#[cfg(target_os = "macos")]
const PAGE: usize = 16384;
#[cfg(target_os = "linux")]
const PAGE: usize = 4096;
impl Attributes {
    fn new(stack_size: Option<usize>) -> crate::io::Result<Attributes> {
        let size = core::cmp::max(stack_size.unwrap_or(DEFAULT_STACK), STACK_MIN);
        let mut attributes = Attributes { words: [0; 8] };
        // SAFETY: the storage is large enough and aligned for pthread_attr_t.
        let error = unsafe { pthread_attr_init(attributes.words.as_mut_ptr()) };
        if error != 0 { return Err(crate::io::Error::from_raw_os_error(error)); }
        // SAFETY: the attributes were initialized above.
        let mut error = unsafe { pthread_attr_setstacksize(attributes.words.as_mut_ptr(), size) };
        if error != 0 {
            // A size that is not a page multiple is rounded up, as std does.
            let rounded = size.checked_add(PAGE - 1).map(|size| size & !(PAGE - 1)).unwrap_or(size);
            // SAFETY: the attributes were initialized above.
            error = unsafe { pthread_attr_setstacksize(attributes.words.as_mut_ptr(), rounded) };
        }
        if error != 0 {
            // SAFETY: the attributes were initialized above.
            unsafe { pthread_attr_destroy(attributes.words.as_mut_ptr()); }
            return Err(crate::io::Error::from_raw_os_error(error));
        }
        Ok(attributes)
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        // SAFETY: an Attributes value always holds initialized attributes.
        unsafe { pthread_attr_destroy(self.words.as_mut_ptr()); }
    }
}

fn start<F: FnOnce() -> T, T>(
    f: F,
    registry: Option<&Threads>,
    name: Option<crate::alloc_crate::string::String>,
    stack_size: Option<usize>,
) -> crate::io::Result<(Arc<Packet<T>>, super::Thread)> {
    let attributes = Attributes::new(stack_size)?;
    let thread = super::Thread::new(name);
    let join = Arc::new(JoinState {
        thread: Mutex::new(None),
        panicked: AtomicBool::new(false),
    });
    let packet = Arc::new(Packet {
        value: UnsafeCell::new(None),
        finished: AtomicBool::new(false),
        join: join.clone(),
    });
    let job = Box::new(Job {
        capture: crate::io::__output_capture(),
        function: f,
        packet: packet.clone(),
        thread: thread.clone(),
    });
    // Scope cleanup must own a join obligation before the worker can start.
    if let Some(registry) = registry {
        registry.registry.lock().unwrap().push(join.clone());
    }
    let mut state = join.thread.lock().unwrap();
    let mut id: NativeThread = 0 as NativeThread;
    let job = Box::into_raw(job);
    let attributes_pointer = attributes.words.as_ptr() as *const u8;
    // SAFETY: callback owns job after successful creation. Scoped callers
    // register a join obligation; detached callers require static ownership.
    let error = unsafe { pthread_create(&mut id, attributes_pointer, run::<F, T>, job as *mut u8) };
    if error != 0 {
        drop(state);
        // SAFETY: failed creation did not transfer job to a native worker.
        unsafe {
            drop(Box::from_raw(job));
        }
        return Err(crate::io::Error::from_raw_os_error(error));
    }
    *state = Some(id);
    drop(state);
    Ok((packet, thread))
}

pub(super) fn spawn_with<F, T>(name: Option<crate::alloc_crate::string::String>, stack_size: Option<usize>, f: F) -> crate::io::Result<JoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let (packet, thread) = start(f, None, name, stack_size)?;
    Ok(JoinHandle { packet, thread })
}

pub fn spawn<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    spawn_with(None, None, f).expect("failed to spawn thread")
}
impl<'scope, 'env> Scope<'scope, 'env> {
    pub fn spawn<F, T>(&'scope self, f: F) -> ScopedJoinHandle<'scope, T>
    where
        F: FnOnce() -> T + Send + 'scope,
        T: Send + 'scope,
    {
        self.spawn_with(None, None, f).expect("failed to spawn thread")
    }
    pub(super) fn spawn_with<F, T>(&'scope self, name: Option<crate::alloc_crate::string::String>, stack_size: Option<usize>, f: F) -> crate::io::Result<ScopedJoinHandle<'scope, T>>
    where
        F: FnOnce() -> T + Send + 'scope,
        T: Send + 'scope,
    {
        let (packet, thread) = start(f, Some(&self.threads), name, stack_size)?;
        Ok(ScopedJoinHandle { packet, thread, scope: PhantomData })
    }
}
impl<T> Drop for JoinHandle<T> {
    fn drop(&mut self) {
        self.packet.join.detach();
    }
}
impl<T> JoinHandle<T> {
    pub fn thread(&self) -> &super::Thread { &self.thread }
    pub(crate) fn native_thread(&self) -> usize {
        self.packet.join.thread.lock().unwrap().map_or(0, |id| id as usize)
    }
    /// Detaches nothing: the native thread stays joinable by its new owner.
    pub(crate) fn into_native_thread(self) -> usize {
        let id = self.packet.join.thread.lock().unwrap().take();
        id.map_or(0, |id| id as usize)
    }
    /// Waits for completion and returns the worker value or its panic payload.
    pub fn join(self) -> super::Result<T> {
        self.packet.join.join();
        // SAFETY: pthread_join finished publication; this consuming handle
        // uniquely extracts the result, leaving the packet empty on Drop.
        self.packet.join.panicked.store(false, Ordering::Release);
        unsafe { (*self.packet.value.get()).take() }.unwrap()
    }
    pub fn is_finished(&self) -> bool {
        self.packet.finished.load(Ordering::Acquire)
    }
}
impl<T> core::fmt::Debug for JoinHandle<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("JoinHandle { .. }")
    }
}
impl<T> core::fmt::Debug for ScopedJoinHandle<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ScopedJoinHandle { .. }")
    }
}

impl<T> ScopedJoinHandle<'_, T> {
    pub fn thread(&self) -> &super::Thread { &self.thread }
    pub fn is_finished(&self) -> bool {
        self.packet.finished.load(Ordering::Acquire)
    }
    /// Waits for completion and consumes the worker result, including a panic.
    pub fn join(self) -> super::Result<T> {
        self.packet.join.join();
        // SAFETY: the worker finished and this consuming handle is the only
        // reader. The scope registry holds only the separate join state.
        self.packet.join.panicked.store(false, Ordering::Release);
        unsafe { (*self.packet.value.get()).take() }.unwrap()
    }
}
