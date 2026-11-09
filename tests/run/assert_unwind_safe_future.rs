use std::cell::Cell;
use std::future::Future;
use std::marker::PhantomPinned;
use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

struct Once<'a> {
    polls: &'a Cell<u32>,
    drops: &'a Cell<u32>,
    address: &'a Cell<usize>,
    _pin: PhantomPinned,
}

impl Future for Once<'_> {
    type Output = u32;

    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<u32> {
        let this = self.as_ref().get_ref();
        let address = this as *const Self as usize;
        if this.address.get() == 0 {
            this.address.set(address);
        }
        assert_eq!(this.address.get(), address);
        this.polls.set(this.polls.get() + 1);
        if this.polls.get() == 1 { Poll::Pending } else { Poll::Ready(42) }
    }
}

impl Drop for Once<'_> {
    fn drop(&mut self) {
        assert_eq!(self.address.get(), self as *const Self as usize);
        self.drops.set(self.drops.get() + 1);
    }
}

fn main() {
    let polls = Cell::new(0);
    let drops = Cell::new(0);
    let address = Cell::new(0usize);
    {
        let mut future = std::pin::pin!(AssertUnwindSafe(Once {
            polls: &polls, drops: &drops, address: &address, _pin: PhantomPinned,
        }));
        let mut context = Context::from_waker(Waker::noop());
        assert_eq!(future.as_mut().poll(&mut context), Poll::Pending);
        assert_eq!(future.as_mut().poll(&mut context), Poll::Ready(42));
    }
    assert_eq!(polls.get(), 2);
    assert_eq!(drops.get(), 1);
    println!("polled {} times, dropped {} time", polls.get(), drops.get());
}
