use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};

static DROPS: AtomicUsize = AtomicUsize::new(0);
struct Item<'a> {
    value: &'a i32,
}
impl Drop for Item<'_> {
    fn drop(&mut self) {
        DROPS.fetch_add(*self.value as usize, Ordering::Relaxed);
    }
}
struct Z;
impl Drop for Z {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::Relaxed);
    }
}
struct Reentrant {
    weak: RefCell<Option<Weak<[Reentrant]>>>,
}
impl Drop for Reentrant {
    fn drop(&mut self) {
        let w = self.weak.borrow_mut().take().unwrap();
        assert!(w.upgrade().is_none());
        // Drop the last explicit weak during payload destruction. The implicit
        // weak must keep the common allocation alive through the remaining drop.
        drop(w);
        DROPS.fetch_add(1, Ordering::Relaxed);
    }
}
#[repr(align(256))]
struct Aligned(u32);

fn covariant<'a, 'b: 'a>(a: Arc<[&'b i32]>) -> Arc<[&'a i32]> {
    a
}
fn weak_covariant<'a, 'b: 'a>(a: Weak<[&'b i32]>) -> Weak<[&'a i32]> {
    a
}
fn main() {
    let x = 3;
    let y = 7;
    let mut values = Vec::with_capacity(20);
    values.push(Item { value: &x });
    values.push(Item { value: &y });
    let a: Arc<[Item<'_>]> = values.into();
    let b = a.clone();
    let weak = Arc::downgrade(&a);
    let weak2 = weak.clone();
    assert!(Arc::ptr_eq(&a, &b));
    assert_eq!(a.len(), 2);
    assert_eq!(*a[1].value, 7);
    drop(a);
    let c = weak.upgrade().unwrap();
    assert!(Arc::ptr_eq(&b, &c));
    drop(b);
    assert_eq!(DROPS.load(Ordering::Relaxed), 0);
    drop(c);
    assert_eq!(DROPS.load(Ordering::Relaxed), 10);
    assert!(weak.upgrade().is_none());
    drop(weak);
    assert!(weak2.upgrade().is_none());
    drop(weak2);
    let empty: Arc<[Item<'_>]> = Vec::new().into();
    assert!(empty.is_empty());
    let weak = Arc::downgrade(&empty);
    drop(empty);
    assert!(weak.upgrade().is_none());
    drop(weak);
    let z: Arc<[Z]> = (0..9).map(|_| Z).collect();
    assert_eq!(z.len(), 9);
    let weak = Arc::downgrade(&z);
    drop(z.clone());
    drop(z);
    assert_eq!(DROPS.load(Ordering::Relaxed), 19);
    assert!(weak.upgrade().is_none());
    drop(weak);
    let a: Arc<[Aligned]> = vec![Aligned(11), Aligned(13)].into();
    assert_eq!(a.as_ptr() as usize % 256, 0);
    assert_eq!(a[0].0 + a[1].0, 24);
    let w = Arc::downgrade(&a);
    drop(a);
    assert!(w.upgrade().is_none());
    drop(w);
    let a: Arc<[i32]> = (0..5).collect();
    assert_eq!(a.iter().copied().sum::<i32>(), 10);
    let a = covariant(Arc::from(vec![&x]));
    let w = weak_covariant(Arc::downgrade(&a));
    assert_eq!(*w.upgrade().unwrap()[0], 3);
    let r: Arc<[Reentrant]> = vec![Reentrant {
        weak: RefCell::new(None),
    }]
    .into();
    *r[0].weak.borrow_mut() = Some(Arc::downgrade(&r));
    drop(r);
    assert_eq!(DROPS.load(Ordering::Relaxed), 20);
    let empty_z: Arc<[Z]> = Vec::new().into();
    drop(empty_z);
    assert_eq!(DROPS.load(Ordering::Relaxed), 20);
    println!("arc slice {} {}", a.len(), DROPS.load(Ordering::Relaxed));
}
