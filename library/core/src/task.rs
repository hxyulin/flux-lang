//! Poll results and reference-counted wake callbacks.
use crate::marker::PhantomData;
use crate::mem::ManuallyDrop;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Poll<T> { Ready(T), Pending }
impl<T> Poll<T> {
    pub const fn is_ready(&self)->bool { matches!(self,Self::Ready(_)) }
    pub const fn is_pending(&self)->bool { !self.is_ready() }
    pub fn map<U,F:FnOnce(T)->U>(self,f:F)->Poll<U> {
        match self { Self::Ready(value)=>Poll::Ready(f(value)), Self::Pending=>Poll::Pending }
    }
}

pub struct RawWaker { data:*const (), vtable:&'static RawWakerVTable }
impl RawWaker {
    pub const fn new(data:*const (),vtable:&'static RawWakerVTable)->Self { Self{data,vtable} }
}
pub struct RawWakerVTable {
    clone:unsafe fn(*const ())->RawWaker,
    wake:unsafe fn(*const ()),
    wake_by_ref:unsafe fn(*const ()),
    drop:unsafe fn(*const ()),
}
impl RawWakerVTable {
    pub const fn new(clone:unsafe fn(*const ())->RawWaker,wake:unsafe fn(*const ()),wake_by_ref:unsafe fn(*const ()),drop:unsafe fn(*const ()))->Self {
        Self{clone,wake,wake_by_ref,drop}
    }
}

pub struct Waker { raw:RawWaker }
// SAFETY: from_raw requires thread-safe callback implementations and data.
unsafe impl Send for Waker {}
// SAFETY: callbacks must support concurrent invocation by the from_raw contract.
unsafe impl Sync for Waker {}
impl Waker {
    /// # Safety
    /// The raw callbacks must obey the RawWaker ownership contract and permit
    /// concurrent access from any thread, including clone, wake and drop.
    pub const unsafe fn from_raw(raw:RawWaker)->Self { Self{raw} }
    pub fn wake(self) {
        let this=ManuallyDrop::new(self);
        // SAFETY: from_raw guarantees the callback consumes this owned reference.
        unsafe { (this.raw.vtable.wake)(this.raw.data); }
    }
    pub fn wake_by_ref(&self) {
        // SAFETY: the callback borrows rather than consuming this reference.
        unsafe { (self.raw.vtable.wake_by_ref)(self.raw.data); }
    }
    pub fn will_wake(&self,other:&Self)->bool {
        self.raw.data==other.raw.data && crate::ptr::eq(self.raw.vtable,other.raw.vtable)
    }
    pub const fn noop()->&'static Waker { &NOOP }
}
impl Clone for Waker {
    fn clone(&self)->Self {
        // SAFETY: the clone callback creates another owned raw reference.
        unsafe { Self::from_raw((self.raw.vtable.clone)(self.raw.data)) }
    }
}
impl Drop for Waker {
    fn drop(&mut self) {
        // SAFETY: this live owned reference has not been consumed by wake.
        unsafe { (self.raw.vtable.drop)(self.raw.data); }
    }
}
impl crate::fmt::Debug for Waker {
    fn fmt(&self,f:&mut crate::fmt::Formatter<'_>)->crate::fmt::Result { f.debug_struct("Waker").finish_non_exhaustive() }
}
unsafe fn noop_clone(_: *const ())->RawWaker { RawWaker::new(crate::ptr::null(),&NOOP_VTABLE) }
unsafe fn noop_callback(_: *const ()) {}
static NOOP_VTABLE:RawWakerVTable=RawWakerVTable::new(noop_clone,noop_callback,noop_callback,noop_callback);
// SAFETY: the callbacks ignore the null data and operate on no shared state.
static NOOP:Waker=unsafe { Waker::from_raw(RawWaker::new(crate::ptr::null(),&NOOP_VTABLE)) };

pub struct Context<'a> { waker:&'a Waker, invariant:PhantomData<&'a mut &'a ()>, not_send_sync:PhantomData<*mut ()> }
impl<'a> Context<'a> {
    pub const fn from_waker(waker:&'a Waker)->Self { Self{waker,invariant:PhantomData,not_send_sync:PhantomData} }
    pub const fn waker(&self)->&'a Waker { self.waker }
}
