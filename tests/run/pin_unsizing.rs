use core::future::Future;
use core::marker::PhantomPinned;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

struct Borrowed<'a> {
    value: &'a u32,
    address: Cell<usize>,
    drops: Rc<Cell<u32>>,
    pinned: PhantomPinned,
}
impl<'a> Future for Borrowed<'a> {
    type Output = &'a u32;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.as_ref().get_ref();
        let address = this as *const Self as usize;
        if this.address.get() == 0 { this.address.set(address); }
        assert_eq!(this.address.get(), address);
        Poll::Ready(this.value)
    }
}
impl Drop for Borrowed<'_> {
    fn drop(&mut self) { self.drops.set(self.drops.get() + 1); }
}
fn boxed<'a, F: Future<Output = &'a u32> + 'a>(value: F) -> Pin<Box<dyn Future<Output = &'a u32> + 'a>> {
    Box::pin(value)
}
trait Read { fn read(&self) -> u32; }
trait Sub: Read {}
impl Read for u32 { fn read(&self) -> u32 { *self } }
impl Sub for u32 {}
fn main() {
    let value = 41;
    let drops = Rc::new(Cell::new(0));
    let make = || Borrowed { value: &value, address: Cell::new(0), drops: drops.clone(), pinned: PhantomPinned };
    let mut cx = Context::from_waker(Waker::noop());
    {
        let mut concrete = Box::pin(make());
        assert_eq!(concrete.as_mut().poll(&mut cx), Poll::Ready(&value));
        let mut object: Pin<Box<dyn Future<Output = &u32> + '_>> = concrete;
        println!("{:?}", object.as_mut().poll(&mut cx));
        let mut direct = boxed(make());
        assert_eq!(direct.as_mut().poll(&mut cx), Poll::Ready(&value));
    }
    assert_eq!(drops.get(), 2);
    let concrete = Box::pin(make());
    let _: Pin<Box<dyn Future<Output = &u32> + '_>> = concrete;
    assert_eq!(drops.get(), 3);
    println!("drops {}", drops.get());
    let mut slice: Pin<Box<[u32]>> = Box::pin([1, 2, 3]);
    slice.as_mut().get_mut()[1] = 9;
    println!("{:?}", slice.as_ref().get_ref());
    let empty: Pin<Box<[Borrowed<'_>]>> = Box::pin([]);
    assert_eq!(empty.as_ref().get_ref().len(), 0);
    let zero: Pin<Box<[(); 3]>> = Box::pin([(); 3]);
    let zero: Pin<Box<[()]>> = zero;
    assert_eq!(zero.as_ref().get_ref().len(), 3);
    {
        let array = [make(), make(), make()];
        let items: Pin<Box<[Borrowed<'_>]>> = Box::pin(array);
        assert_eq!(items.as_ref().get_ref().len(), 3);
    }
    assert_eq!(drops.get(), 6);
    let mut n = 17u32;
    let concrete = Pin::new(&mut n);
    let pinned: Pin<&mut dyn Read> = concrete;
    println!("{}", pinned.as_ref().get_ref().read());
    let concrete = Pin::new(&mut n);
    let shared: Pin<&dyn Read> = concrete;
    assert_eq!(shared.get_ref().read(), 17);
    let sub: Pin<Box<dyn Sub>> = Box::pin(23u32);
    let base: Pin<Box<dyn Read>> = sub;
    assert_eq!(base.as_ref().get_ref().read(), 23);
    let owner = Arc::new(29u32);
    // SAFETY: an Arc allocation stays at its address while owned.
    let concrete = unsafe { Pin::new_unchecked(owner.clone()) };
    let object: Pin<Arc<dyn Read>> = concrete;
    assert_eq!(object.as_ref().get_ref().read(), 29);
    drop(object);
    assert_eq!(Arc::strong_count(&owner), 1);
    let owner = Rc::new([4u32, 5, 6]);
    // SAFETY: an Rc allocation stays at its address while owned.
    let concrete = unsafe { Pin::new_unchecked(owner.clone()) };
    let object: Pin<Rc<[u32]>> = concrete;
    assert_eq!(object.as_ref().get_ref(), &[4u32, 5, 6]);
    drop(object);
    assert_eq!(Rc::strong_count(&owner), 1);
    println!("drops {}", drops.get());
}
