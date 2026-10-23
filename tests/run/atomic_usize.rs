use std::sync::atomic::{AtomicUsize, Ordering, fence};
use Ordering::{Acquire, AcqRel, Relaxed, Release, SeqCst};

const INITIAL: AtomicUsize = AtomicUsize::new(17);

fn shared<T: Send + Sync>(_: &T) {}

fn exercise(a: &AtomicUsize, order: Ordering) {
    assert_eq!(a.fetch_add(3, order), 17);
    assert_eq!(a.fetch_sub(3, order), 20);
}

fn pair(a: &AtomicUsize, success: Ordering, failure: Ordering) {
    a.store(17, Relaxed);
    assert_eq!(a.compare_exchange(17, 29, success, failure), Ok(17));
    assert_eq!(a.compare_exchange(17, 31, success, failure), Err(29));
    assert_eq!(a.compare_exchange_weak(29, 17, success, failure), Ok(29));
    assert_eq!(a.compare_exchange_weak(29, 31, success, failure), Err(17));
}

fn main() {
    let a = INITIAL;
    shared(&a);
    assert_eq!(std::mem::size_of::<AtomicUsize>(), 8);
    assert_eq!(std::mem::align_of::<AtomicUsize>(), 8);
    for order in [Relaxed, Acquire, Release, AcqRel, SeqCst] {
        exercise(&a, order);
        for failure in [Relaxed, Acquire, SeqCst] {
            pair(&a, order, failure);
        }
    }
    for order in [Relaxed, Release, SeqCst] {
        a.store(42, order);
        for load in [Relaxed, Acquire, SeqCst] {
            assert_eq!(a.load(load), 42);
        }
    }
    for order in [Acquire, Release, AcqRel, SeqCst] {
        fence(order);
    }
    a.store(usize::MAX, Relaxed);
    assert_eq!(a.fetch_add(1, Relaxed), usize::MAX);
    assert_eq!(a.fetch_sub(1, Relaxed), 0);
    assert_eq!(a.load(Relaxed), usize::MAX);
    let mut exclusive = AtomicUsize::new(3);
    *exclusive.get_mut() = 11;
    assert_eq!(exclusive.into_inner(), 11);
    println!("atomic usize: all orders, fifteen CAS pairs, old values and wrapping passed");
}
