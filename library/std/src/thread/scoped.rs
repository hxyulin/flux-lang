//! Native workers whose borrowed environments stay alive until every join.

use crate::alloc_crate::boxed::Box;
use crate::alloc_crate::sync::Arc;
use crate::alloc_crate::vec::Vec;
use crate::sync::Mutex;
use core::cell::UnsafeCell;
use core::marker::{PhantomData, Send, Sync};
use core::ops::{Drop, FnOnce};

#[cfg(target_os = "macos")]
type NativeThread = *mut u8;
#[cfg(not(target_os = "macos"))]
type NativeThread = usize;

unsafe extern "C" {
    fn pthread_create(thread: *mut NativeThread, attr: *const u8,
        callback: extern "C" fn(*mut u8) -> *mut u8, arg: *mut u8) -> i32;
    fn pthread_join(thread: NativeThread, result: *mut *mut u8) -> i32;
    fn abort() -> !;
}

struct JoinState {
    thread: Mutex<Option<NativeThread>>,
}

// SAFETY: native identifiers are accessed only under the mutex and joined once.
unsafe impl Send for JoinState {}
// SAFETY: the mutex serializes even concurrent calls to join.
unsafe impl Sync for JoinState {}

impl JoinState {
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
    let scope = Scope { threads: Arc::new(Threads { registry: Mutex::new(Vec::new()) }), scope: PhantomData, env: PhantomData };
    f(&scope)
}

struct Packet<T> {
    value: UnsafeCell<Option<T>>,
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
    unsafe { *job.packet.value.get() = Some(value); }
    core::ptr::null_mut()
}

pub struct ScopedJoinHandle<'scope, T> {
    packet: Arc<Packet<T>>,
    scope: PhantomData<&'scope mut &'scope ()>,
}

impl<'scope, 'env> Scope<'scope, 'env> {
    pub fn spawn<F, T>(&'scope self, f: F) -> ScopedJoinHandle<'scope, T>
    where
        F: FnOnce() -> T + Send + 'scope,
        T: Send + 'scope,
    {
        let join = Arc::new(JoinState { thread: Mutex::new(None) });
        let packet = Arc::new(Packet { value: UnsafeCell::new(None), join: join.clone() });
        let job = Box::into_raw(Box::new(Job { function: f, packet: packet.clone() }));
        // Register before starting the worker. Scope cleanup can never lose a
        // successfully created thread, including when its handle is forgotten.
        self.threads.registry.lock().unwrap().push(join.clone());
        let mut state = join.thread.lock().unwrap();
        let mut id: NativeThread = 0 as NativeThread;
        // SAFETY: the callback owns job, and the scope joins before F's borrows
        // expire. The attribute pointer selects the native default attributes.
        let error = unsafe { pthread_create(&mut id, core::ptr::null(), run::<F, T>, job as *mut u8) };
        if error != 0 {
            drop(state);
            // SAFETY: failed creation did not transfer job to a worker.
            unsafe { drop(Box::from_raw(job)); }
            panic!("cannot spawn scoped thread");
        }
        *state = Some(id);
        drop(state);
        ScopedJoinHandle { packet, scope: PhantomData }
    }
}

impl<T> ScopedJoinHandle<'_, T> {
    /// Flux panics abort, so a joined worker always returns its value.
    pub fn join(self) -> Result<T, core::convert::Infallible> {
        self.packet.join.join();
        // SAFETY: the worker finished and this consuming handle is the only
        // reader. The scope registry holds only the separate join state.
        Ok(unsafe { (*self.packet.value.get()).take() }.unwrap())
    }
}
