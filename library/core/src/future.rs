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
