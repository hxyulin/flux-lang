use core::pin::Pin;
use core::task::{RawWaker, RawWakerVTable, Waker};
use core::sync::atomic::{AtomicUsize, Ordering};
use core::mem::ManuallyDrop;
use std::sync::{Arc, Mutex, RwLock};

#[derive(Debug)]
struct M<T: ?Sized>(Mutex<T>);
#[derive(Debug)]
struct R<T: ?Sized>(RwLock<T>);
#[derive(Debug)]
struct Record<T: ?Sized> { label: u32, tail: T }

trait Buffer { fn bytes(self: Pin<&mut Self>, other: &[u8]) -> &[u8]; }
struct Bytes([u8; 3]);
impl Buffer for Bytes {
    fn bytes(self: Pin<&mut Self>, _: &[u8]) -> &[u8] { &self.get_mut().0 }
}
trait Action { fn amount() -> usize; }
struct One;
struct Three;
impl Action for One { fn amount() -> usize { 1 } }
impl Action for Three { fn amount() -> usize { 3 } }
fn table<T: Action>() -> &'static RawWakerVTable {
    &RawWakerVTable::new(clone::<T>, wake::<T>, by_ref::<T>, release)
}
unsafe fn clone<T: Action>(data: *const ()) -> RawWaker {
    // SAFETY: every raw data pointer here owns an Arc reference.
    let value = ManuallyDrop::new(unsafe { Arc::<AtomicUsize>::from_raw(data as *const AtomicUsize) });
    RawWaker::new(Arc::into_raw(Arc::clone(&value)) as *const (), table::<T>())
}
unsafe fn wake<T: Action>(data: *const ()) {
    // SAFETY: this callback consumes the raw Arc reference once.
    let value = unsafe { Arc::<AtomicUsize>::from_raw(data as *const AtomicUsize) };
    value.fetch_add(T::amount(), Ordering::SeqCst);
}
unsafe fn by_ref<T: Action>(data: *const ()) {
    // SAFETY: ManuallyDrop retains the borrowed Arc reference count.
    let value = ManuallyDrop::new(unsafe { Arc::<AtomicUsize>::from_raw(data as *const AtomicUsize) });
    value.fetch_add(T::amount(), Ordering::SeqCst);
}
unsafe fn release(data: *const ()) {
    // SAFETY: each raw Arc reference is consumed by wake or release once.
    drop(unsafe { Arc::<AtomicUsize>::from_raw(data as *const AtomicUsize) });
}
fn waker<T: Action>(value: &Arc<AtomicUsize>) -> Waker {
    let raw = RawWaker::new(Arc::into_raw(Arc::clone(value)) as *const (), table::<T>());
    // SAFETY: all callbacks use thread-safe Arc/atomic operations.
    unsafe { Waker::from_raw(raw) }
}
fn main() {
    let mut iter = vec![1u32, 2, 3].into_iter();
    assert_eq!(iter.next(), Some(1));
    fn sync<T: Sync>(_: &T) {}
    sync(&iter);
    println!("{}", std::thread::spawn(move || iter.sum::<u32>()).join().unwrap());
    println!("{:?} {:?}", M(Mutex::new(7u32)), R(RwLock::new(11u32)));
    fn describe<T: ?Sized + core::fmt::Debug>(value: &Record<T>) { println!("{:?}", value); }
    describe(&Record { label: 1, tail: [2u32, 3] });
    let mut bytes = Bytes([4, 5, 6]);
    println!("{:?}", Pin::new(&mut bytes).bytes(&[0]));
    let count = Arc::new(AtomicUsize::new(0));
    for use_three in [false, true] {
        let waker = if use_three { waker::<Three>(&count) } else { waker::<One>(&count) };
        waker.clone().wake();
        waker.wake_by_ref();
    }
    assert_eq!(count.load(Ordering::SeqCst), 8);
    assert_eq!(Arc::strong_count(&count), 1);
    println!("wakes {}", count.load(Ordering::SeqCst));
}
