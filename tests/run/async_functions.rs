use core::future::Future;
use core::task::{Context, Poll, Waker};
use core::pin::{Pin, pin};
use core::cell::{Cell, RefCell};

struct Yield(bool);
impl Future for Yield {
    type Output = ();
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        if this.0 { this.0 = false; Poll::Pending } else { Poll::Ready(()) }
    }
}
fn run<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    loop { if let Poll::Ready(value) = future.as_mut().poll(&mut cx) { return value; } }
}
async fn boxed_recursion(n: u32) -> u32 {
    if n == 0 { return 0; }
    Box::pin(boxed_recursion(n - 1)).await + 1
}
async fn construct_only() { drop(construct_only()); }
async fn number() -> u8 { 42 }
async fn identity<T>(value: T) -> T { Yield(true).await; value }
async fn borrowed<'a, T>(value: &'a T) -> &'a T { Yield(true).await; value }
async fn opaque() -> impl Iterator<Item = u32> { core::iter::once(42u32) }
async fn early(value: u32) -> Result<u32, u32> {
    Yield(true).await;
    let value = if value == 0 { Err(7u32) } else { Ok(value) }?;
    return Ok(value + 1);
}
struct S(u32);
impl S {
    async fn get(&self) -> &u32 { Yield(true).await; &self.0 }
    async fn add(&mut self, n: u32) { Yield(true).await; self.0 += n; }
    async fn take(self) -> u32 { self.0 }
}
trait Manual { async fn value() -> u32; }
impl Manual for S { fn value() -> impl Future<Output = u32> { async { 42 } } }
trait Explicit { fn explicit() -> impl Future<Output = u32>; }
impl Explicit for S { async fn explicit() -> u32 { 42 } }
trait Read {
    async fn read(&self) -> u32;
    async fn incremented(&self) -> u32 { self.read().await + 1 }
}
impl Read for S { async fn read(&self) -> u32 { Yield(true).await; self.0 } }
async unsafe fn dereference(pointer: *const u32) -> u32 { unsafe { *pointer } }

struct Guard<'a>(u32, &'a RefCell<Vec<u32>>);
impl Drop for Guard<'_> { fn drop(&mut self) { self.1.borrow_mut().push(self.0); } }
async fn arguments<'a>(
    (first, _): (Guard<'a>, Guard<'a>),
    _unused: Guard<'a>,
    _: Guard<'a>,
    starts: &'a Cell<u32>,
) {
    starts.set(starts.get() + 1);
    Yield(true).await;
    drop(first);
}
async fn ref_pattern(ref value: u32) -> u32 { Yield(true).await; *value }
fn cancellation(polls: u32) {
    let log = RefCell::new(Vec::new());
    let starts = Cell::new(0);
    let mut future = Box::pin(arguments((Guard(1, &log), Guard(2, &log)), Guard(3, &log), Guard(4, &log), &starts));
    assert!(log.borrow().is_empty());
    assert_eq!(starts.get(), 0);
    let mut cx = Context::from_waker(Waker::noop());
    for i in 0..polls {
        let result = future.as_mut().poll(&mut cx);
        assert_eq!(result.is_pending(), i == 0);
    }
    drop(future);
    println!("{} {} {:?}", polls, starts.get(), *log.borrow());
}
fn main() {
    assert_eq!(run(number()), 42u8);
    assert_eq!(run(boxed_recursion(8)), 8);
    run(construct_only());
    assert_eq!(run(S::value()), 42);
    assert_eq!(run(S::explicit()), 42);
    assert_eq!(run(identity(String::from("owned"))), "owned");
    let n = 7;
    assert_eq!(*run(borrowed(&n)), 7);
    assert_eq!(run(opaque()).next(), Some(42));
    assert_eq!(run(early(0)), Err(7));
    assert_eq!(run(early(41)), Ok(42));
    let mut s = S(40);
    assert_eq!(*run(s.get()), 40);
    run(s.add(1));
    assert_eq!(run(s.incremented()), 42);
    assert_eq!(run(s.take()), 41);
    assert_eq!(run(unsafe { dereference(&n) }), 7);
    assert_eq!(run(ref_pattern(9)), 9);
    for polls in 0..=2 { cancellation(polls); }
    println!("async functions passed");
}
