struct Guard<'a>(&'a core::cell::Cell<u32>);
impl Drop for Guard<'_> { fn drop(&mut self) { self.0.set(self.0.get()+1); } }
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicUsize,Ordering};
use core::task::{Context,Poll,RawWaker,RawWakerVTable,Waker};
static CLONES:AtomicUsize=AtomicUsize::new(0);
static WAKES:AtomicUsize=AtomicUsize::new(0);
static REFS:AtomicUsize=AtomicUsize::new(0);
static DROPS:AtomicUsize=AtomicUsize::new(0);
unsafe fn clone_raw(_: *const ())->RawWaker {
    CLONES.fetch_add(1,Ordering::SeqCst);
    RawWaker::new(core::ptr::null(),&VTABLE)
}
unsafe fn wake_raw(_: *const ()) {WAKES.fetch_add(1,Ordering::SeqCst);}
unsafe fn wake_ref(_: *const ()) {REFS.fetch_add(1,Ordering::SeqCst);}
unsafe fn drop_raw(_: *const ()) {DROPS.fetch_add(1,Ordering::SeqCst);}
static VTABLE:RawWakerVTable=RawWakerVTable::new(clone_raw,wake_raw,wake_ref,drop_raw);
struct Once { value:u32,pending:bool }
impl Future for Once {
    type Output=u32;
    fn poll(self:Pin<&mut Self>,context:&mut Context<'_>)->Poll<u32> {
        let this=self.get_mut();
        if this.pending {this.pending=false;context.waker().wake_by_ref();Poll::Pending}
        else {Poll::Ready(this.value)}
    }
}
struct AddressGuard<'a> {
    address: &'a core::cell::Cell<usize>,
    drops: &'a core::cell::Cell<u32>,
    _pin: core::marker::PhantomPinned,
}
impl Drop for AddressGuard<'_> {
    fn drop(&mut self) {
        assert_eq!(self as *const Self as usize, self.address.get());
        self.drops.set(self.drops.get() + 1);
    }
}
fn main() {
    let drops=core::cell::Cell::new(0);
    {
        let guard=Guard(&drops);
        let pinned=core::pin::pin!(guard);
        assert_eq!(drops.get(),0);
        drop(pinned);
        assert_eq!(drops.get(),0);
    }
    assert_eq!(drops.get(),1);
    let address=core::cell::Cell::new(0usize);
    let address_drops=core::cell::Cell::new(0u32);
    {
        let mut slot=core::pin::pin!(Some(AddressGuard{address:&address,drops:&address_drops,_pin:core::marker::PhantomPinned}));
        address.set(slot.as_ref().as_pin_ref().unwrap().get_ref() as *const AddressGuard<'_> as usize);
        slot.set(None);
        assert_eq!(address_drops.get(),1);
    }
    assert_eq!(address_drops.get(),1);
    let immovable=core::pin::pin!((7u32,core::marker::PhantomPinned));
    assert_eq!(immovable.as_ref().get_ref().0,7);
    // SAFETY: callbacks retain only static atomics, accept null data, and are
    // thread safe. Each wake/drop consumes exactly one logical raw reference.
    let waker=unsafe {Waker::from_raw(RawWaker::new(core::ptr::null(),&VTABLE))};
    let cloned=waker.clone();
    assert!(waker.will_wake(&cloned));
    assert!(!waker.will_wake(Waker::noop()));
    cloned.wake();
    let mut context=Context::from_waker(&waker);
    let mut future=Once{value:42,pending:true};
    let mut future=Pin::new(&mut future);
    assert!(future.as_mut().poll(&mut context).is_pending());
    assert_eq!(Future::poll(future.as_mut(),&mut context),Poll::Ready(42));
    let mut value=7u32;
    let mut pinned=Pin::new(&mut value);
    pinned.set(9);
    assert_eq!(*pinned.as_ref().get_ref(),9);
    let mut pair=(3u32,4u32);
    let pair=Pin::new(&mut pair);
    // SAFETY: this projection does not move either field.
    let mut field=unsafe {pair.map_unchecked_mut(|pair| &mut pair.1)};
    *field.as_mut().get_mut()=11;
    assert_eq!(*field.as_ref().get_ref(),11);
    let mut lanes=core::pin::pin!([Once{value:3,pending:true},Once{value:5,pending:false}]);
    let mut result=0;
    for i in 0..2 {
        // SAFETY: the array remains pinned and each projection retains the
        // selected element's location through every poll and its destruction.
        let mut lane=unsafe {lanes.as_mut().map_unchecked_mut(|lanes| &mut lanes[i])};
        loop { match lane.as_mut().poll(&mut context) {Poll::Ready(value)=>{result+=value;break;},Poll::Pending=>{}} }
    }
    assert_eq!(result,8);
    let mut slot=core::pin::pin!(Some(Once{value:13,pending:false}));
    let mut noop=Context::from_waker(Waker::noop());
    assert!(slot.as_ref().as_pin_ref().is_some());
    assert_eq!(slot.as_mut().as_pin_mut().unwrap().poll(&mut noop),Poll::Ready(13));
    slot.set(None);
    assert!(slot.as_mut().as_pin_mut().is_none());
    Waker::noop().clone().wake();
    drop(waker);
    assert_eq!(CLONES.load(Ordering::SeqCst),1);
    assert_eq!(WAKES.load(Ordering::SeqCst),1);
    assert_eq!(REFS.load(Ordering::SeqCst),2);
    assert_eq!(DROPS.load(Ordering::SeqCst),1);
    println!("future polling 42, wake ownership 1 1 2 1");
}
