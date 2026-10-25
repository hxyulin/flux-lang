use std::sync::atomic::{AtomicU64, Ordering::*};

fn main() {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let mut local = AtomicU64::from(u64::MAX);
    assert_eq!(local.fetch_add(1, Relaxed), u64::MAX);
    assert_eq!(local.fetch_sub(1, AcqRel), 0);
    assert_eq!(local.compare_exchange(0, 7, Acquire, Relaxed), Err(u64::MAX));
    assert_eq!(local.compare_exchange(u64::MAX, 7, SeqCst, Acquire), Ok(u64::MAX));
    while local.compare_exchange_weak(7, 9, Release, Relaxed).is_err() {}
    *local.get_mut() = 13;
    assert_eq!(local.into_inner(), 13);
    let empty = AtomicU64::default();
    assert_eq!(format!("{:?}", empty), "0");
    std::thread::scope(|scope| {
        for _ in 0..8 { scope.spawn(|| { for _ in 0..1000 { COUNT.fetch_add(1, Relaxed); } }); }
    });
    assert_eq!(COUNT.load(SeqCst), 8000);
    println!("atomic64 {}", COUNT.load(Acquire));
}
