// `std::task::Wake`: a waker built from an `Arc`, with its default
// `wake_by_ref`, an overridden one, clones and drops.
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Wake, Waker};

struct Counter {
    wakes: AtomicUsize,
}

impl Wake for Counter {
    fn wake(self: Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }
}

struct ByRef {
    wakes: AtomicUsize,
    by_ref: AtomicUsize,
}

impl Wake for ByRef {
    fn wake(self: Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.by_ref.fetch_add(1, Ordering::SeqCst);
    }
}

struct Twice {
    polls: u32,
}

impl Future for Twice {
    type Output = u32;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u32> {
        self.polls += 1;
        if self.polls < 3 {
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(self.polls)
        }
    }
}

fn main() {
    let counter = Arc::new(Counter { wakes: AtomicUsize::new(0) });
    let waker = Waker::from(counter.clone());
    println!("strong after from: {}", Arc::strong_count(&counter));
    waker.wake_by_ref();
    let second = waker.clone();
    println!("strong after clone: {}", Arc::strong_count(&counter));
    println!("will_wake: {}", waker.will_wake(&second));
    second.wake();
    println!("strong after wake: {}", Arc::strong_count(&counter));
    drop(waker);
    println!("strong after drop: {}", Arc::strong_count(&counter));
    println!("wakes: {}", counter.wakes.load(Ordering::SeqCst));

    let by_ref = Arc::new(ByRef { wakes: AtomicUsize::new(0), by_ref: AtomicUsize::new(0) });
    let waker: Waker = by_ref.clone().into();
    let mut cx = Context::from_waker(&waker);
    let mut future = Twice { polls: 0 };
    let mut pinned = Pin::new(&mut future);
    let result = loop {
        if let Poll::Ready(value) = pinned.as_mut().poll(&mut cx) {
            break value;
        }
    };
    println!("ready after {} polls", result);
    println!(
        "by_ref: {} wake: {}",
        by_ref.by_ref.load(Ordering::SeqCst),
        by_ref.wakes.load(Ordering::SeqCst)
    );
    drop(waker);
    println!("strong at end: {}", Arc::strong_count(&by_ref));
}
