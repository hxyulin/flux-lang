use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

static PRIMARY_DROPS: AtomicUsize = AtomicUsize::new(0);
static SECONDARY_DROPS: AtomicUsize = AtomicUsize::new(0);
static LEAF_DROPS: AtomicUsize = AtomicUsize::new(0);

struct Primary;
struct Secondary;
struct Leaf;

thread_local! {
    static MAIN: Cell<bool> = const { Cell::new(false) };
    static OVERRIDE: Cell<u32> = { panic!("set must skip this initializer"); };
    static LATE: Cell<u32> = const { Cell::new(0) };
    static PRIMARY: Primary = Primary;
    static SECONDARY: Secondary = Secondary;
    static LEAF: Leaf = Leaf;
}

impl Drop for Primary {
    fn drop(&mut self) {
        assert!(PRIMARY.try_with(|_| ()).is_err());
        assert_eq!(OVERRIDE.get(), 19);
        assert_eq!(LATE.try_with(|value| value.get()).unwrap_or(999), 19);
        PRIMARY_DROPS.fetch_add(1, Ordering::SeqCst);
        // Registration during cleanup must append to the existing list,
        // without registering another platform TLS callback.
        SECONDARY.with(|_| ());
    }
}

impl Drop for Secondary {
    fn drop(&mut self) {
        assert!(PRIMARY.try_with(|_| ()).is_err());
        assert!(SECONDARY.try_with(|_| ()).is_err());
        SECONDARY_DROPS.fetch_add(1, Ordering::SeqCst);
        LEAF.with(|_| ());
    }
}

impl Drop for Leaf {
    fn drop(&mut self) {
        assert!(PRIMARY.try_with(|_| ()).is_err());
        assert!(SECONDARY.try_with(|_| ()).is_err());
        assert!(LEAF.try_with(|_| ()).is_err());
        LEAF_DROPS.fetch_add(1, Ordering::SeqCst);
        if MAIN.get() {
            assert_eq!(PRIMARY_DROPS.load(Ordering::SeqCst), 4);
            assert_eq!(SECONDARY_DROPS.load(Ordering::SeqCst), 4);
            assert_eq!(LEAF_DROPS.load(Ordering::SeqCst), 4);
            println!("main tls dropped 4 4 4");
        }
    }
}

fn initialize() {
    OVERRIDE.set(7);
    assert_eq!(OVERRIDE.get(), 7);
    OVERRIDE.set(19);
    PRIMARY.with(|_| ());
    LATE.set(19);
}

fn main() {
    MAIN.set(true);
    initialize();
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..3 { handles.push(scope.spawn(initialize)); }
        for handle in handles { handle.join().unwrap(); }
    });
    assert_eq!(PRIMARY_DROPS.load(Ordering::SeqCst), 3);
    assert_eq!(SECONDARY_DROPS.load(Ordering::SeqCst), 3);
    assert_eq!(LEAF_DROPS.load(Ordering::SeqCst), 3);
    println!("workers 3 3 3");
}
