use std::cell::Cell;
use std::collections::{BTreeMap, btree_map::Entry};

struct Owned<'a>(&'a Cell<usize>, i32);
impl Drop for Owned<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

fn main() {
    let mut map = BTreeMap::new();
    for n in 0..257 {
        map.insert((n * 73) % 257, n);
    }
    assert_eq!(map.len(), 257);
    for n in (0..257).step_by(3) {
        assert!(map.remove(&n).is_some());
    }
    for n in (0..257).step_by(3) {
        assert_eq!(map.insert(n, n * 2), None);
    }
    let mut old = -1;
    for (&key, _) in &map {
        assert!(key > old);
        old = key;
    }
    assert_eq!(map.first_key_value().map(|(&k, _)| k), Some(0));
    assert_eq!(map.keys().len(), 257);
    assert_eq!(map.values().len(), 257);
    let copy = map.clone();
    assert_eq!(map, copy);
    let mut it = map.keys();
    assert_eq!(it.next(), Some(&0));
    assert_eq!(it.clone().count(), 256);
    let sum: i32 = map.values().copied().sum();
    let calls = Cell::new(0);
    map.entry(4).or_insert_with(|| {
        calls.set(calls.get() + 1);
        900
    });
    assert_eq!(calls.get(), 0);
    map.entry(900).or_insert_with_key(|key| *key + 1);
    map.entry(901)
        .and_modify(|_| panic!("vacant callback"))
        .or_default();
    map.entry(900).and_modify(|v| *v += 1);
    assert_eq!(map[&900], 902);
    match map.entry(900) {
        Entry::Occupied(mut e) => {
            assert_eq!(*e.key(), 900);
            assert_eq!(e.insert(77), 902);
            *e.into_mut() += 1;
        }
        Entry::Vacant(_) => panic!("expected occupied"),
    }
    match map.entry(999) {
        Entry::Vacant(e) => assert_eq!(e.into_key(), 999),
        Entry::Occupied(_) => panic!("expected vacant"),
    }
    let mut names: BTreeMap<String, i32> = [(String::from("alpha"), 7), (String::from("beta"), 8)]
        .into_iter()
        .collect();
    assert_eq!(names.remove("alpha"), Some(7));
    assert_eq!(
        names.first_key_value().map(|(k, _)| k.as_str()),
        Some("beta")
    );
    let removed = names.remove_entry("beta").unwrap();
    assert_eq!(removed.0, "beta");
    assert!(names.is_empty());

    let drops = Cell::new(0);
    let mut owned = BTreeMap::new();
    for n in 0..15 {
        owned.insert(n, Owned(&drops, n));
    }
    let removed = owned.remove(&7).unwrap();
    assert_eq!(removed.1, 7);
    assert_eq!(drops.get(), 0);
    drop(removed);
    assert_eq!(drops.get(), 1);
    owned.entry(2).or_insert(Owned(&drops, 90));
    assert_eq!(drops.get(), 2);
    let mut iter = owned.into_iter();
    drop(iter.next());
    assert_eq!(drops.get(), 3);
    drop(iter);
    assert_eq!(drops.get(), 16);
    for n in 0..257 {
        assert!(map.remove(&n).is_some());
    }
    assert_eq!(map.len(), 2);
    assert_eq!(map.remove(&900), Some(78));
    assert_eq!(map.remove(&901), Some(0));
    assert!(map.is_empty());
    assert!(map.iter().next().is_none());
    map.insert(5, 6);
    assert_eq!(map.first_key_value(), Some((&5, &6)));
    println!("btree {sum} {old} {}", drops.get());
}
