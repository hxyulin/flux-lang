use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering::SeqCst},
};
static DROPS: AtomicUsize = AtomicUsize::new(0);
struct Owner(u32);
impl Drop for Owner {
    fn drop(&mut self) {
        DROPS.fetch_add(1, SeqCst);
    }
}
#[repr(align(128))]
struct Aligned(Owner);
struct Zst;
impl Drop for Zst {
    fn drop(&mut self) {
        DROPS.fetch_add(1, SeqCst);
    }
}
fn main() {
    let a = Arc::new(Owner(7));
    let b = a.clone();
    let a = Arc::try_unwrap(a).err().unwrap();
    assert!(Arc::ptr_eq(&a, &b));
    assert_eq!(Arc::strong_count(&a), 2);
    drop(b);
    let weak = Arc::downgrade(&a);
    let value = Arc::try_unwrap(a).ok().unwrap();
    assert_eq!(value.0, 7);
    assert!(weak.upgrade().is_none());
    assert_eq!(DROPS.load(SeqCst), 0);
    drop(value);
    drop(weak);
    assert_eq!(DROPS.load(SeqCst), 1);
    let value = Arc::try_unwrap(Arc::new(Aligned(Owner(9)))).ok().unwrap();
    assert_eq!(value.0.0, 9);
    drop(value);
    drop(Arc::try_unwrap(Arc::new(Zst)).ok().unwrap());
    assert_eq!(DROPS.load(SeqCst), 3);
    for index in 0..64 {
        let value = Arc::new(Owner(index));
        let weak = Arc::downgrade(&value);
        let gate = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            let gate = &gate;
            let first = scope.spawn(move || {
                gate.fetch_add(1, SeqCst);
                while gate.load(SeqCst) != 2 {
                    std::hint::spin_loop();
                }
                match Arc::try_unwrap(value) {
                    Ok(v) => {
                        assert_eq!(v.0, index);
                        drop(v)
                    }
                    Err(v) => drop(v),
                }
            });
            let second = scope.spawn(move || {
                gate.fetch_add(1, SeqCst);
                while gate.load(SeqCst) != 2 {
                    std::hint::spin_loop();
                }
                if let Some(v) = weak.upgrade() {
                    assert_eq!(v.0, index);
                    drop(v);
                }
            });
            first.join().unwrap();
            second.join().unwrap();
        });
        assert_eq!(DROPS.load(SeqCst), index as usize + 4);
    }
    println!("unwrap 64 races {} drops", DROPS.load(SeqCst));
}
