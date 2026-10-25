use std::cell::Cell;
use std::sync::{Arc, Weak};

struct Value<'a> { clones: &'a Cell<usize>, drops: &'a Cell<usize>, n: usize }
impl Clone for Value<'_> {
    fn clone(&self) -> Self {
        self.clones.set(self.clones.get() + 1);
        Self { clones: self.clones, drops: self.drops, n: self.n }
    }
}
impl Drop for Value<'_> { fn drop(&mut self) { self.drops.set(self.drops.get() + 1); } }
struct NoClone;
fn main() {
    let empty: Arc<[NoClone]> = Default::default(); assert!(empty.is_empty());
    let slice = Arc::<[u32]>::from(&[1u32, 2, 3][..]);
    assert_eq!(slice.as_ref(), [1, 2, 3]);
    let clones = Cell::new(0); let drops = Cell::new(0);
    {
        let mut value = Arc::new(Value { clones: &clones, drops: &drops, n: 3 });
        Arc::get_mut(&mut value).unwrap().n += 1;
        assert_eq!(clones.get(), 0);
        let weak = Arc::downgrade(&value);
        assert!(Arc::get_mut(&mut value).is_none());
        Arc::make_mut(&mut value).n += 1;
        assert!(weak.upgrade().is_none()); assert_eq!(clones.get(), 0); assert_eq!(drops.get(), 0);
        let old = value.clone();
        assert!(Arc::get_mut(&mut value).is_none());
        Arc::make_mut(&mut value).n += 1;
        assert_eq!(old.n, 5); assert_eq!(value.n, 6); assert_eq!(clones.get(), 1);
    }
    assert_eq!(drops.get(), 2);
    {
        let values = vec![Value { clones: &clones, drops: &drops, n: 7 }, Value { clones: &clones, drops: &drops, n: 11 }];
        let mut values: Arc<[Value<'_>]> = values.into();
        let weak: Weak<[Value<'_>]> = Arc::downgrade(&values);
        Arc::make_mut(&mut values)[0].n = 13;
        assert!(weak.upgrade().is_none()); assert_eq!(clones.get(), 1);
        let old = values.clone();
        Arc::make_mut(&mut values)[1].n = 17;
        assert_eq!(old[1].n, 11); assert_eq!(values[1].n, 17); assert_eq!(clones.get(), 3);
        assert_eq!(drops.get(), 2);
    }
    assert_eq!(drops.get(), 6);
    for _ in 0..64 {
        let mut owner = Arc::new(42usize);
        let weak = Arc::downgrade(&owner);
        std::thread::scope(|scope| {
            scope.spawn(move || {
                for _ in 0..100 {
                    if let Some(old) = weak.upgrade() { assert_eq!(*old, 42); }
                }
            });
            *Arc::make_mut(&mut owner) = 7;
        });
        assert_eq!(*owner, 7);
    }
    println!("arc uniqueness {} {}", clones.get(), drops.get());
}
