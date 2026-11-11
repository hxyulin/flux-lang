use std::sync::{Barrier, atomic::{AtomicUsize, Ordering}};

static SINGLE: Barrier = Barrier::new(1);

fn main() {
    assert!(Barrier::new(0).wait().is_leader());
    assert!(SINGLE.wait().is_leader());
    assert!(SINGLE.wait().is_leader());
    assert_eq!(format!("{:?}", SINGLE), "Barrier { .. }");
    assert_eq!(format!("{:?}", SINGLE.wait()), "BarrierWaitResult { is_leader: true }");
    let barrier = Barrier::new(4);
    let leaders = [const { AtomicUsize::new(0) }; 32];
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                for round in 0..32 {
                    if barrier.wait().is_leader() {
                        leaders[round].fetch_add(1, Ordering::SeqCst);
                    }
                }
            });
        }
    });
    for leader in &leaders { assert_eq!(leader.load(Ordering::SeqCst), 1); }
    println!("reusable barrier passed");
}
