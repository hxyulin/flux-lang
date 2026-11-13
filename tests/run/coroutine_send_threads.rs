use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Waker};

struct Guard(Arc<AtomicUsize>);
impl Drop for Guard {
    fn drop(&mut self) { self.0.fetch_add(1, Ordering::SeqCst); }
}
struct YieldOnce {
    pending: bool,
    polls: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
}
impl Future for YieldOnce {
    type Output = ();
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        this.polls.fetch_add(1, Ordering::SeqCst);
        if this.pending {
            this.pending = false;
            context.waker().wake_by_ref();
            Poll::Pending
        } else { Poll::Ready(()) }
    }
}
impl Drop for YieldOnce {
    fn drop(&mut self) { self.drops.fetch_add(1, Ordering::SeqCst); }
}
fn future(guard: Guard, polls: Arc<AtomicUsize>, drops: Arc<AtomicUsize>) -> impl Future<Output = u32> + Send {
    async move {
        let guard = guard;
        let saved = Box::new(41u32);
        YieldOnce { pending: true, polls, drops }.await;
        let value = *saved + 1;
        drop(guard);
        value
    }
}
fn suspend<F: Future + Send + 'static>(future: F) -> Pin<Box<F>> {
    std::thread::spawn(move || {
        let mut future = Box::pin(future);
        let mut context = Context::from_waker(Waker::noop());
        assert!(future.as_mut().poll(&mut context).is_pending());
        future
    }).join().unwrap()
}
fn finish<F: Future<Output = u32> + Send + 'static>(mut future: Pin<Box<F>>) -> u32 {
    std::thread::spawn(move || {
        let mut context = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("second poll must complete"),
        }
    }).join().unwrap()
}
fn main() {
    let guards = Arc::new(AtomicUsize::new(0));
    let polls = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let suspended = suspend(future(Guard(guards.clone()), polls.clone(), drops.clone()));
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    assert_eq!(guards.load(Ordering::SeqCst), 0);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert_eq!(finish(suspended), 42);
    assert_eq!(polls.load(Ordering::SeqCst), 2);
    assert_eq!(guards.load(Ordering::SeqCst), 1);
    assert_eq!(drops.load(Ordering::SeqCst), 1);

    // Cancellation after Pending drops the initialized guard and awaited future.
    let suspended = suspend(future(Guard(guards.clone()), polls.clone(), drops.clone()));
    assert_eq!(polls.load(Ordering::SeqCst), 3);
    assert_eq!(guards.load(Ordering::SeqCst), 1);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    std::thread::spawn(move || drop(suspended)).join().unwrap();
    assert_eq!(guards.load(Ordering::SeqCst), 2);
    assert_eq!(drops.load(Ordering::SeqCst), 2);

    // Before the first poll, cancellation drops captures without entering the body.
    let unpolled = future(Guard(guards.clone()), polls.clone(), drops.clone());
    std::thread::spawn(move || drop(unpolled)).join().unwrap();
    assert_eq!(polls.load(Ordering::SeqCst), 3);
    assert_eq!(guards.load(Ordering::SeqCst), 3);
    assert_eq!(drops.load(Ordering::SeqCst), 2);
    println!("coroutine Send threads: 42 3 3 2");
}
