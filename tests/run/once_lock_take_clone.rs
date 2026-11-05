use std::cell::Cell;
use std::sync::OnceLock;

struct Counted<'a> {
    drops: &'a Cell<usize>,
    value: u32,
}
impl<'a> Clone for Counted<'a> {
    fn clone(&self) -> Self {
        Counted { drops: self.drops, value: self.value + 100 }
    }
}
impl<'a> Drop for Counted<'a> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

fn main() {
    let drops = Cell::new(0usize);
    let mut lock: OnceLock<Counted<'_>> = OnceLock::new();
    assert!(lock.take().is_none());
    assert!(lock.get_mut().is_none());
    let empty_copy = lock.clone();
    assert!(empty_copy.get().is_none());

    lock.get_or_init(|| Counted { drops: &drops, value: 7 });
    lock.get_mut().unwrap().value += 1;
    let copy = lock.clone();
    assert_eq!(copy.get().map(|c| c.value), Some(108));

    let taken = lock.take().unwrap();
    assert_eq!(taken.value, 8);
    assert!(lock.get().is_none());
    assert_eq!(drops.get(), 0);
    drop(taken);
    assert_eq!(drops.get(), 1);

    assert_eq!(lock.get_or_init(|| Counted { drops: &drops, value: 9 }).value, 9);
    assert_eq!(lock.into_inner().map(|c| c.value), Some(9));
    assert_eq!(drops.get(), 2);
    assert_eq!(copy.into_inner().map(|c| c.value), Some(108));
    assert_eq!(drops.get(), 3);

    let number = OnceLock::new();
    number.set(5u64).unwrap();
    let cloned = number.clone();
    println!("{:?} {:?} {}", number.get(), cloned.get(), drops.get());
}
