use std::sync::atomic::{AtomicU8, Ordering::*};
#[repr(C)]
struct Guarded {
    before: u8,
    atomic: AtomicU8,
    after: u8,
}
fn main() {
    let guarded = Guarded {
        before: 0xAB,
        atomic: AtomicU8::new(0),
        after: 0xCD,
    };
    let a = &guarded.atomic;
    for success in [Relaxed, Acquire, Release, AcqRel, SeqCst] {
        for failure in [Relaxed, Acquire, SeqCst] {
            for value in 0u8..=255 {
                a.store(value, Relaxed);
                assert_eq!(
                    a.compare_exchange(value, value.wrapping_add(1), success, failure),
                    Ok(value)
                );
                assert_eq!(
                    a.compare_exchange(value, 0, success, failure),
                    Err(value.wrapping_add(1))
                );
                loop {
                    if a.compare_exchange_weak(value.wrapping_add(1), value, success, failure)
                        .is_ok()
                    {
                        break;
                    }
                }
            }
        }
    }
    let counter = AtomicU8::new(0);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let counter = &counter;
            scope.spawn(move || {
                for _ in 0..1000 {
                    let mut old = counter.load(Relaxed);
                    loop {
                        match counter.compare_exchange_weak(
                            old,
                            old.wrapping_add(1),
                            AcqRel,
                            Acquire,
                        ) {
                            Ok(_) => break,
                            Err(actual) => old = actual,
                        }
                    }
                }
            });
        }
    });
    assert_eq!(counter.load(SeqCst), (8000usize % 256) as u8);
    assert_eq!(guarded.before, 0xAB);
    assert_eq!(guarded.after, 0xCD);
    println!("byte cas {}", counter.load(Relaxed));
}
