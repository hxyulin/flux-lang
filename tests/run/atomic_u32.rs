use Ordering::{AcqRel, Acquire, Relaxed, Release, SeqCst};
use std::sync::atomic::{AtomicU32, Ordering};

const INITIAL: AtomicU32 = AtomicU32::new(17);
#[repr(C)]
struct Guarded {
    before: u32,
    counter: AtomicU32,
    after: u32,
}
// Selected counter excerpts, retaining the actual fields, method signatures,
// and counter expressions from flux-intern/src/lib.rs and flux-ty/src/ty.rs.
// The surrounding interner algorithms and sources are not compiled here.
mod symbol_counter_probe {
    use std::sync::atomic::{AtomicU32, Ordering};
    struct Symbol(u32);
    struct Interner {
        next: AtomicU32,
    }
    static INTERNER: Interner = Interner {
        next: AtomicU32::new(0),
    };
    struct Stats {
        symbols: usize,
    }
    fn stats() -> Stats {
        Stats {
            symbols: INTERNER.next.load(Ordering::Relaxed) as usize,
        }
    }
    impl Interner {
        fn intern(&self, s: &str, hash: u64) -> Symbol {
            let _ = (s, hash);
            let index = self.next.fetch_add(1, Ordering::Relaxed);
            assert!(index < u32::MAX, "more than u32::MAX symbols interned");
            let symbol = Symbol(index);
            symbol
        }
    }
    pub fn exercise() {
        assert_eq!(INTERNER.intern("first", 0).0, 0);
        assert_eq!(INTERNER.intern("second", 1).0, 1);
        assert_eq!(stats().symbols, 2);
    }
}
mod type_counter_probe {
    use std::sync::atomic::{AtomicU32, Ordering};
    struct Ty(u32);
    enum TyKind {
        Unit,
    }
    const EMPTY: Ty = Ty(u32::MAX);
    struct Interner {
        next: AtomicU32,
    }
    impl Interner {
        fn intern(&self, kind: TyKind, hash: u64) -> u32 {
            let _ = (kind, hash);
            let id = self.next.fetch_add(1, Ordering::Relaxed);
            assert!(id < EMPTY.0, "more than u32::MAX types interned");
            id
        }
    }
    pub fn exercise() {
        let interner = Interner {
            next: AtomicU32::new(0),
        };
        assert_eq!(interner.intern(TyKind::Unit, 0), 0);
        assert_eq!(interner.intern(TyKind::Unit, 1), 1);
    }
}
fn shared<T: Send + Sync>(_: &T) {}
fn exercise(a: &AtomicU32, order: Ordering) {
    a.store(u32::MAX, Relaxed);
    assert_eq!(a.fetch_add(1, order), u32::MAX);
    assert_eq!(a.fetch_add(17, order), 0);
    assert_eq!(a.load(Relaxed), 17);
}
fn main() {
    let a = INITIAL;
    shared(&a);
    assert_eq!(std::mem::size_of::<AtomicU32>(), 4);
    assert_eq!(std::mem::align_of::<AtomicU32>(), 4);
    assert_eq!(std::mem::size_of::<Guarded>(), 12);
    assert_eq!(std::mem::align_of::<Guarded>(), 4);
    let g = Guarded {
        before: 0xaabbccdd,
        counter: AtomicU32::new(0),
        after: 0x11223344,
    };
    for order in [Relaxed, Acquire, Release, AcqRel, SeqCst] {
        exercise(&g.counter, order);
        assert_eq!(g.before, 0xaabbccdd);
        assert_eq!(g.after, 0x11223344);
    }
    for store in [Relaxed, Release, SeqCst] {
        a.store(42, store);
        for load in [Relaxed, Acquire, SeqCst] {
            assert_eq!(a.load(load), 42);
        }
    }
    let counters = [AtomicU32::new(9), AtomicU32::new(31), AtomicU32::new(77)];
    assert_eq!(std::mem::size_of::<[AtomicU32; 3]>(), 12);
    assert_eq!(std::mem::align_of::<[AtomicU32; 3]>(), 4);
    assert_eq!(
        (&counters[1] as *const AtomicU32 as usize) - (&counters[0] as *const AtomicU32 as usize),
        4
    );
    exercise(&counters[1], AcqRel);
    assert_eq!(counters[0].load(Relaxed), 9);
    assert_eq!(counters[2].load(Relaxed), 77);
    let mut exclusive = AtomicU32::new(3);
    *exclusive.get_mut() = 11;
    assert_eq!(exclusive.into_inner(), 11);
    symbol_counter_probe::exercise();
    type_counter_probe::exercise();
    println!(
        "atomic u32: exact width, layout, orders, wrapping, old values and consumer counters passed"
    );
}
