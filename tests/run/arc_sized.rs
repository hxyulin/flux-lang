use std::cell::{Cell, RefCell};
use std::sync::{Arc, Weak};

struct Counted<'a>(&'a Cell<usize>, usize);
impl Drop for Counted<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() + self.1);
    }
}
#[repr(align(64))]
struct Aligned(u64);
struct Z;
impl Drop for Z {
    fn drop(&mut self) {
        println!("z dropped");
    }
}

struct Reentrant {
    weak: RefCell<Option<Weak<Reentrant>>>,
}
impl Drop for Reentrant {
    fn drop(&mut self) {
        let weak = self.weak.get_mut().take().unwrap();
        assert!(weak.upgrade().is_none());
        drop(weak);
        println!("reentrant dropped");
    }
}

fn main() {
    let reentrant = Arc::new(Reentrant {
        weak: RefCell::new(None),
    });
    *reentrant.weak.borrow_mut() = Some(Arc::downgrade(&reentrant));
    drop(reentrant);
    let drops = Cell::new(0);
    for _ in 0..100 {
        let a = Arc::new(Counted(&drops, 1));
        let b = a.clone();
        assert!(Arc::ptr_eq(&a, &b));
        let w = Arc::downgrade(&a);
        let w2 = w.clone();
        drop(a);
        let c = w.upgrade().unwrap();
        assert!(Arc::ptr_eq(&b, &c));
        drop((b, c));
        assert!(w.upgrade().is_none());
        assert!(w2.upgrade().is_none());
    }
    assert_eq!(drops.get(), 100);
    let empty = Weak::<Counted<'_>>::new();
    assert!(empty.clone().upgrade().is_none());
    let aligned = Arc::new(Aligned(42));
    assert_eq!((&*aligned as *const Aligned as usize) % 64, 0);
    assert_eq!(aligned.0, 42);
    let z = Arc::new(Z);
    let weak = Arc::downgrade(&z);
    drop(z.clone());
    drop(z);
    assert!(weak.upgrade().is_none());
    let value = 17;
    let reference = Arc::new(&value);
    assert_eq!(**reference, 17);
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Arc<usize>>();
    send_sync::<Weak<usize>>();
    println!(
        "drops {} aligned {} borrowed {}",
        drops.get(),
        aligned.0,
        **reference
    );
}
