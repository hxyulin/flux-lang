use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use std::cell::Cell;
use std::rc::Rc;

unsafe trait Representation<'a, T>: 'a {
    fn into_raw(self) -> *mut (dyn Future<Output = T> + 'a);
    unsafe fn release(ptr: *mut (dyn Future<Output = T> + 'a));
}

// SAFETY: the raw pointer owns its allocation until release consumes it.
unsafe impl<'a, T, F: Future<Output = T> + 'a> Representation<'a, T> for Box<F> {
    fn into_raw(self) -> *mut (dyn Future<Output = T> + 'a) {
        Box::into_raw(self)
    }
    unsafe fn release(ptr: *mut (dyn Future<Output = T> + 'a)) {
        // SAFETY: the caller returns the pointer produced by into_raw once.
        drop(unsafe { Box::from_raw(ptr) });
    }
}

struct Local<'a, T> {
    ptr: *mut (dyn Future<Output = T> + 'a),
    release: unsafe fn(*mut (dyn Future<Output = T> + 'a)),
}
impl<'a, T> Local<'a, T> {
    fn new<F: Representation<'a, T>>(value: F) -> Self {
        Self { ptr: value.into_raw(), release: F::release }
    }
    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<T> {
        // SAFETY: the allocation stays pinned until this object is dropped.
        unsafe { Pin::new_unchecked(&mut *self.ptr).poll(cx) }
    }
}
impl<T> Drop for Local<'_, T> {
    fn drop(&mut self) {
        // SAFETY: this object uniquely owns its raw allocation.
        unsafe { (self.release)(self.ptr) }
    }
}

struct Borrowed<'a> { value: &'a u32, drops: Rc<Cell<u32>> }
impl<'a> Future for Borrowed<'a> {
    type Output = &'a u32;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(self.get_mut().value)
    }
}
impl Drop for Borrowed<'_> {
    fn drop(&mut self) { self.drops.set(self.drops.get() + 1); }
}
fn main() {
    let value = 37;
    let drops = Rc::new(Cell::new(0));
    let mut cx = Context::from_waker(Waker::noop());
    {
        let mut local = Local::new(Box::new(Borrowed { value: &value, drops: drops.clone() }));
        assert_eq!(local.poll(&mut cx), Poll::Ready(&value));
        println!("{:?}", local.poll(&mut cx));
    }
    assert_eq!(drops.get(), 1);
    println!("drops {}", drops.get());
}
