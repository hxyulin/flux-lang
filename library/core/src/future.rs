//! Values that produce an output when polled to completion.
use crate::pin::Pin;
use crate::task::{Context, Poll};

#[lang = "future_trait"]
pub trait Future {
    type Output;
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output>;
}

// Async blocks own a stable allocation. The compiler specializes these
// operations from the closure's checked MIR, including its cancellation paths.
#[lang = "coroutine"]
pub(crate) struct Coroutine<C,R> {
    state:*mut u8,
    marker:crate::marker::PhantomData<(C,R,*mut ())>,
    pin:crate::marker::PhantomPinned,
}
#[lang = "coroutine_new"]
#[rustc_intrinsic]
pub(crate) fn __coroutine_new<C:FnOnce()->R,R>(closure:C)->Coroutine<C,R> { loop {} }
#[lang = "coroutine_await"]
#[rustc_intrinsic]
pub(crate) fn __coroutine_await<F:Future>(future:&mut F)->F::Output { loop {} }
#[rustc_intrinsic]
fn __coroutine_poll<C:FnOnce()->R,R>(state:*mut u8,context:&mut Context<'_>)->Poll<R> { loop {} }
#[rustc_intrinsic]
fn __coroutine_drop<C,R>(state:*mut u8) { loop {} }
impl<C:FnOnce()->R,R> Future for Coroutine<C,R> {
    type Output=R;
    fn poll(self:Pin<&mut Self>,context:&mut Context<'_>)->Poll<R> {
        // SAFETY: reading the allocation pointer does not move pinned storage.
        let state=unsafe {self.get_unchecked_mut()}.state;
        __coroutine_poll::<C,R>(state,context)
    }
}
impl<C,R> Drop for Coroutine<C,R> {
    fn drop(&mut self) { __coroutine_drop::<C,R>(self.state); }
}
mod libc {
    unsafe extern "C" {
        pub fn posix_memalign(pointer:*mut *mut u8,align:usize,size:usize)->i32;
        pub fn free(pointer:*mut u8);
        pub fn abort()->!;
    }
}
#[lang = "coroutine_alloc"]
fn coroutine_alloc(size:usize,align:usize)->*mut u8 {
    let mut pointer=crate::ptr::null_mut();
    let align=if align<crate::mem::size_of::<usize>() {crate::mem::size_of::<usize>()} else {align};
    // SAFETY: the generated layout has power-of-two alignment, and pointer is
    // a writable result slot. Allocation failure terminates before any access.
    unsafe {
        if libc::posix_memalign(&mut pointer,align,size)!=0 {libc::abort();}
    }
    pointer
}
#[lang = "coroutine_free"]
fn coroutine_free(pointer:*mut u8) {
    // SAFETY: only generated coroutine destruction calls this operation, once
    // after dropping the live fields of its posix_memalign allocation.
    unsafe {libc::free(pointer);}
}

impl<F: Future + ?Sized + crate::marker::Unpin> Future for &mut F {
    type Output = F::Output;
    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<F::Output> {
        F::poll(Pin::new(&mut **self), context)
    }
}
impl<P: crate::ops::DerefMut> Future for Pin<P> where P::Target: Future {
    type Output = <<P as crate::ops::Deref>::Target as Future>::Output;
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: the outer pin keeps the inner pinned pointer in place.
        unsafe { self.get_unchecked_mut() }.as_mut().poll(context)
    }
}

/// Conversion into a future, which `.await` applies.
pub trait IntoFuture {
    type Output;
    type IntoFuture: Future<Output = Self::Output>;
    fn into_future(self) -> Self::IntoFuture;
}
impl<F: Future> IntoFuture for F {
    type Output = F::Output;
    type IntoFuture = F;
    fn into_future(self) -> F { self }
}

/// A future that is immediately ready with a value.
#[derive(Debug, Clone)]
pub struct Ready<T>(crate::option::Option<T>);
impl<T> crate::marker::Unpin for Ready<T> {}
impl<T> Future for Ready<T> {
    type Output = T;
    fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<T> {
        Poll::Ready(self.0.take().expect("`Ready` polled after completion"))
    }
}
impl<T> Ready<T> {
    pub fn into_inner(self) -> T { self.0.expect("Called `into_inner()` on `Ready` after completion") }
}
/// A future that is immediately ready with `t`.
pub fn ready<T>(t: T) -> Ready<T> { Ready(crate::option::Option::Some(t)) }

/// A future that never completes.
pub struct Pending<T> { marker: crate::marker::PhantomData<fn() -> T> }
pub fn pending<T>() -> Pending<T> { Pending { marker: crate::marker::PhantomData } }
impl<T> Future for Pending<T> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<T> { Poll::Pending }
}
impl<T> crate::fmt::Debug for Pending<T> {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result { f.debug_struct("Pending").finish() }
}
impl<T> crate::clone::Clone for Pending<T> {
    fn clone(&self) -> Self { pending() }
}

/// A future whose poll calls `f`.
pub struct PollFn<F> { f: F }
impl<F: crate::marker::Unpin> crate::marker::Unpin for PollFn<F> {}
pub fn poll_fn<T, F: FnMut(&mut Context<'_>) -> Poll<T>>(f: F) -> PollFn<F> { PollFn { f } }
impl<T, F: FnMut(&mut Context<'_>) -> Poll<T>> Future for PollFn<F> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<T> {
        // SAFETY: the closure is not structurally pinned; it is only called.
        (unsafe { &mut self.get_unchecked_mut().f })(context)
    }
}
impl<F> crate::fmt::Debug for PollFn<F> {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result { f.debug_struct("PollFn").finish() }
}
