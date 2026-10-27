use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU8, Ordering::*};
#[repr(C)]
struct Surrounded {
    before: u8,
    atomic: AtomicU8,
    after: u8,
}
struct Publication {
    ready: AtomicU8,
    data: UnsafeCell<[u64; 2]>,
}
// SAFETY: the one producer/consumer handshake below transfers exclusive
// access to data through release/acquire stores and loads of ready.
unsafe impl Sync for Publication {}
fn main() {
    assert_eq!(std::mem::size_of::<AtomicU8>(), 1);
    assert_eq!(std::mem::align_of::<AtomicU8>(), 1);
    assert_eq!(std::mem::size_of::<Surrounded>(), 3);
    let canary = Surrounded {
        before: 0xa5,
        atomic: AtomicU8::new(0),
        after: 0x5a,
    };
    for value in 0..=255u8 {
        canary.atomic.store(value, Relaxed);
        assert_eq!(canary.atomic.load(Relaxed), value);
        assert_eq!(canary.before, 0xa5);
        assert_eq!(canary.after, 0x5a);
    }
    canary.atomic.store(37, Release);
    assert_eq!(canary.atomic.load(Acquire), 37);
    canary.atomic.store(49, SeqCst);
    assert_eq!(canary.atomic.load(SeqCst), 49);
    let mut owned = AtomicU8::from(7);
    *owned.get_mut() = 9;
    assert_eq!(owned.into_inner(), 9);
    assert_eq!(AtomicU8::default().load(Relaxed), 0);
    assert_eq!(format!("{:?}", canary.atomic), "49");
    let publication = Publication {
        ready: AtomicU8::new(0),
        data: UnsafeCell::new([0, 0]),
    };
    let publication = &publication;
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for value in 1..=2000u64 {
                while publication.ready.load(Acquire) != 0 {}
                // SAFETY: observing zero gives producer exclusive data access.
                unsafe {
                    *publication.data.get() = [value, value * 3];
                }
                publication.ready.store(1, Release);
            }
        });
        scope.spawn(|| {
            for value in 1..=2000u64 {
                while publication.ready.load(Acquire) != 1 {}
                // SAFETY: observing one gives consumer exclusive data access.
                let data = unsafe { *publication.data.get() };
                assert_eq!(data, [value, value * 3]);
                publication.ready.store(0, Release);
            }
        });
    });
    assert_eq!(publication.ready.load(SeqCst), 0);
    println!("atomic u8 1 1 256 2000");
}
