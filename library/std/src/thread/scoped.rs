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
    fn abort() -> !;
}

struct JoinState {
    thread: Mutex<Option<NativeThread>>,
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
        loop {
            let next = self.registry.lock().unwrap().pop();
            match next {
                Some(thread) => thread.join(),
                None => break,
            }
        }
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
    value: UnsafeCell<Option<T>>,
    finished: AtomicBool,
    join: Arc<JoinState>,
}

// SAFETY: the worker alone writes the value; the handle reads only after join.
unsafe impl<T: Send> Send for Packet<T> {}
// SAFETY: joining synchronizes publication and the handle is consumed on read.
unsafe impl<T: Send> Sync for Packet<T> {}

struct Job<F, T> {
    function: F,
    packet: Arc<Packet<T>>,
}

extern "C" fn run<F: FnOnce() -> T, T>(argument: *mut u8) -> *mut u8 {
    // SAFETY: spawn transferred precisely one Box<Job<F, T>> to this callback.
    let job = unsafe { *Box::from_raw(argument as *mut Job<F, T>) };
    let value = (job.function)();
    // SAFETY: the worker has sole access until pthread_join completes.
    unsafe {
        *job.packet.value.get() = Some(value);
    }
    job.packet.finished.store(true, Ordering::Release);
    core::ptr::null_mut()
}

pub struct ScopedJoinHandle<'scope, T> {
    packet: Arc<Packet<T>>,
    scope: PhantomData<&'scope mut &'scope ()>,
}

/// An owned native worker. Dropping its handle detaches without waiting.
pub struct JoinHandle<T> {
    packet: Arc<Packet<T>>,
}

fn start<F: FnOnce() -> T, T>(f: F, registry: Option<&Threads>) -> Arc<Packet<T>> {
    let join = Arc::new(JoinState {
        thread: Mutex::new(None),
    });
    let packet = Arc::new(Packet {
        value: UnsafeCell::new(None),
        finished: AtomicBool::new(false),
        join: join.clone(),
    });
    let job = Box::new(Job {
        function: f,
        packet: packet.clone(),
    });
    // Scope cleanup must own a join obligation before the worker can start.
    if let Some(registry) = registry {
        registry.registry.lock().unwrap().push(join.clone());
    }
    let mut state = join.thread.lock().unwrap();
    let mut id: NativeThread = 0 as NativeThread;
    let job = Box::into_raw(job);
    // SAFETY: callback owns job after successful creation. Scoped callers
    // register a join obligation; detached callers require static ownership.
    let error = unsafe { pthread_create(&mut id, core::ptr::null(), run::<F, T>, job as *mut u8) };
    if error != 0 {
        drop(state);
        // SAFETY: failed creation did not transfer job to a native worker.
        unsafe {
            drop(Box::from_raw(job));
        }
        panic!("cannot spawn thread");
    }
    *state = Some(id);
    drop(state);
    packet
}

pub fn spawn<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    JoinHandle {
        packet: start(f, None),
    }
}
impl<'scope, 'env> Scope<'scope, 'env> {
    pub fn spawn<F, T>(&'scope self, f: F) -> ScopedJoinHandle<'scope, T>
    where
        F: FnOnce() -> T + Send + 'scope,
        T: Send + 'scope,
    {
        ScopedJoinHandle {
            packet: start(f, Some(&self.threads)),
            scope: PhantomData,
        }
    }
}
impl<T> Drop for JoinHandle<T> {
    fn drop(&mut self) {
        self.packet.join.detach();
    }
}
impl<T> JoinHandle<T> {
    /// Panics abort in Flux; successful native joins therefore return Ok.
    pub fn join(self) -> super::Result<T> {
        self.packet.join.join();
        // SAFETY: pthread_join finished publication; this consuming handle
        // uniquely extracts the result, leaving the packet empty on Drop.
        Ok(unsafe { (*self.packet.value.get()).take() }.unwrap())
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
    pub fn is_finished(&self) -> bool {
        self.packet.finished.load(Ordering::Acquire)
    }
    /// Flux panics abort, so a joined worker always returns its value.
    pub fn join(self) -> super::Result<T> {
        self.packet.join.join();
        // SAFETY: the worker finished and this consuming handle is the only
        // reader. The scope registry holds only the separate join state.
        Ok(unsafe { (*self.packet.value.get()).take() }.unwrap())
    }
}
