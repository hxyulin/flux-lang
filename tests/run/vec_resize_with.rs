use std::cell::Cell;

struct Value<'a> {
    id: usize,
    drops: &'a Cell<usize>,
}

impl Drop for Value<'_> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
        println!("drop {}", self.id);
    }
}

#[repr(align(64))]
struct Zst;

impl Drop for Zst {
    fn drop(&mut self) {
        println!("drop zst");
    }
}

fn generate() -> u32 {
    23
}

fn main() {
    let mut values = Vec::with_capacity(2);
    values.push(11u32);
    let mut calls = 0;
    values.resize_with(7, || {
        calls += 1;
        calls * 3
    });
    assert_eq!(values, [11, 3, 6, 9, 12, 15, 18]);
    assert_eq!(calls, 6);
    println!("grown {:?} {}", values, values.capacity());
    let capacity = values.capacity();
    let pointer = values.as_ptr();
    values.resize_with(7, || panic!("equal size called factory"));
    values.resize_with(2, || panic!("shrink called factory"));
    assert_eq!(values.capacity(), capacity);
    assert_eq!(values.as_ptr(), pointer);
    values.resize_with(5, generate);
    assert_eq!(values, [11, 3, 23, 23, 23]);
    println!("regrown {:?} {}", values, values.capacity());

    let drops = Cell::new(0);
    let mut next = 0;
    let mut owners = Vec::new();
    owners.resize_with(4, || {
        next += 1;
        Value { id: next, drops: &drops }
    });
    assert_eq!(drops.get(), 0);
    owners.resize_with(2, || panic!("shrink owners called factory"));
    assert_eq!(drops.get(), 2);
    owners.resize_with(3, || Value { id: 5, drops: &drops });
    assert_eq!(owners[2].id, 5);
    drop(owners);
    assert_eq!(drops.get(), 5);
    println!("owner drops {}", drops.get());

    let mut zsts = Vec::new();
    let mut zst_calls = 0;
    zsts.resize_with(3, || { zst_calls += 1; Zst });
    assert_eq!(zst_calls, 3);
    assert_eq!(zsts.capacity(), usize::MAX);
    assert_eq!(zsts.as_ptr() as usize % 64, 0);
    zsts.resize_with(1, || panic!("shrink zsts called factory"));
    drop(zsts);
    println!("zst calls {}", zst_calls);

    // ArenaMap's two growth shapes: descending prefix and sparse upper slot.
    let mut slots: Vec<Option<String>> = vec![Some(String::from("kept"))];
    slots.resize_with(4, || None);
    slots.rotate_right(3);
    assert_eq!(slots[3].as_ref().unwrap(), "kept");
    slots.resize_with(9, || None);
    slots[8] = Some(String::from("last"));
    println!("slots {:?}", slots);
    let mut empty: Vec<String> = Vec::new();
    empty.resize_with(0, || panic!("empty called factory"));
    assert_eq!(empty.capacity(), 0);
}
