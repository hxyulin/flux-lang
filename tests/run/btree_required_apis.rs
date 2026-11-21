use std::borrow::Borrow;
use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};
use std::hash::{Hash, Hasher};
struct Trace(Vec<u8>);
impl Hasher for Trace {
    fn write(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }
    fn finish(&self) -> u64 {
        self.0.len() as u64
    }
}
struct Key<'a> {
    rank: u32,
    tag: u32,
    drops: &'a Cell<usize>,
}
impl PartialEq for Key<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.rank == other.rank
    }
}
impl Eq for Key<'_> {}
impl PartialOrd for Key<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Key<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rank.cmp(&other.rank)
    }
}
impl Borrow<u32> for Key<'_> {
    fn borrow(&self) -> &u32 {
        &self.rank
    }
}
impl Drop for Key<'_> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
struct Value<'a>(&'a Cell<usize>, u32);
impl Drop for Value<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
fn main() {
    let mut set_hash = Trace(Vec::new());
    BTreeSet::from([9u16, 2, 9, 5]).hash(&mut set_hash);
    let mut expected_hash = Trace(Vec::new());
    expected_hash.write_usize(3);
    for value in [2u16, 5, 9] {
        value.hash(&mut expected_hash);
    }
    assert_eq!(set_hash.0, expected_hash.0);

    let mut debug_entries = BTreeMap::from([(1, "one")]);
    let occupied = debug_entries.entry(1);
    assert_eq!(
        format!("{occupied:?}"),
        "Entry(OccupiedEntry { key: 1, value: \"one\" })",
    );
    match debug_entries.entry(2) {
        Entry::Vacant(entry) => assert_eq!(format!("{entry:?}"), "VacantEntry(2)"),
        Entry::Occupied(_) => panic!("expected a vacant entry"),
    }
    // Debug for a vacant entry only needs the key to implement Debug.
    struct NoDebug;
    let mut map_with_opaque_values = BTreeMap::<u8, NoDebug>::new();
    if let Entry::Vacant(entry) = map_with_opaque_values.entry(7) {
        assert_eq!(format!("{entry:?}"), "VacantEntry(7)");
    }

    let mut map = BTreeMap::new();
    for n in 0..257u32 {
        map.insert((n * 73) % 257, n);
    }
    for n in (0..257u32).step_by(4) {
        map.remove(&n);
    }
    for n in (0..257u32).step_by(4) {
        map.insert(n, n + 1000);
    }
    let mut iter = map.iter();
    let mut front = 0u32;
    let mut back = 256u32;
    for n in 0..257 {
        let key = if n % 2 == 0 {
            let k = front;
            front += 1;
            iter.next().unwrap().0
        } else {
            let k = back;
            back -= 1;
            assert_eq!(*iter.clone().next_back().unwrap().0, k);
            iter.next_back().unwrap().0
        };
        let expected = if n % 2 == 0 { front - 1 } else { back + 1 };
        assert_eq!(*key, expected);
    }
    assert!(iter.next().is_none());
    assert!(iter.next_back().is_none());
    assert_eq!(iter.len(), 0);
    let mut it = map.iter_mut();
    let (a, av) = it.next().unwrap();
    let (z, zv) = it.next_back().unwrap();
    let (b, bv) = it.next().unwrap();
    assert_eq!((*a, *b, *z), (0, 1, 256));
    *av = 71;
    *bv = 73;
    *zv = 79;
    drop(it);
    assert_eq!((map[&0], map[&1], map[&256]), (71, 73, 79));
    for (_, v) in &mut map {
        *v += 1;
    }
    let small = BTreeMap::from([(3u32, 30u64), (1, 10), (2, 20)]);
    let mut hash = Trace(Vec::new());
    small.hash(&mut hash);
    let mut expected = Trace(Vec::new());
    3usize.hash(&mut expected);
    for item in [(1u32, 10u64), (2, 20), (3, 30)] {
        item.hash(&mut expected);
    }
    assert_eq!(hash.0, expected.0);
    let mut it = small.iter();
    it.next();
    let mut keys = small.keys();
    keys.next_back();
    let mut values = small.values();
    values.next();
    println!("views {:?} {:?} {:?}", it, keys, values);
    println!("pretty {:#?}", small.iter());
    let mut owned = small.clone().into_iter();
    owned.next();
    owned.next_back();
    println!("owned {:?}", owned);
    let mut owned_values = small.clone().into_values();
    owned_values.next_back();
    println!("owned values {:?}", owned_values);
    let mut mutable = small.clone();
    let mut it = mutable.iter_mut();
    let (_, v) = it.next().unwrap();
    *v = 99;
    println!("mut {:?}", it);
    drop(it);
    assert_eq!(mutable[&1], 99);
    let mut values = mutable.values_mut();
    values.next_back();
    println!("mut values {:?}", values);
    let names = BTreeMap::from([(String::from("alpha"), 7), (String::from("beta"), 9)]);
    let (key, value) = names.get_key_value("alpha").unwrap();
    assert_eq!((key.as_str(), *value), ("alpha", 7));
    let mut range_map = BTreeMap::new();
    for key in [8, 2, 7, 0, 1, 9, 6, 5, 3, 4] {
        range_map.insert(key, key);
    }
    range_map.remove(&3);
    range_map.insert(11, 11);
    let mut range = range_map.range_mut(2..=6);
    let (first, a) = range.next().unwrap();
    let (last, b) = range.next_back().unwrap();
    let (middle, c) = range.next().unwrap();
    assert_eq!((*first, *last, *middle), (2, 6, 4));
    *a += 20;
    *b += 60;
    *c += 40;
    assert_eq!(range.next_back().map(|(key, _)| *key), Some(5));
    assert!(range.next().is_none());
    assert!(range.next_back().is_none());
    let upper = range_map.split_off(&5);
    assert_eq!(range_map.into_iter().collect::<Vec<_>>(), [(0, 0), (1, 1), (2, 22), (4, 44)]);
    assert_eq!(upper.keys().copied().collect::<Vec<_>>(), [5, 6, 7, 8, 9, 11]);
    assert_eq!(upper[&6], 66);
    let mut names = names;
    let mut upper_names = names.split_off("alpha");
    assert!(names.is_empty());
    assert!(upper_names.split_off("z").is_empty());
    assert_eq!(upper_names.pop_last().unwrap().0, "beta");
    let kd = Cell::new(0);
    let vd = Cell::new(0);
    let mut left = BTreeMap::new();
    left.insert(
        Key {
            rank: 1,
            tag: 10,
            drops: &kd,
        },
        Value(&vd, 1),
    );
    left.insert(
        Key {
            rank: 2,
            tag: 20,
            drops: &kd,
        },
        Value(&vd, 2),
    );
    let mut right = BTreeMap::new();
    right.insert(
        Key {
            rank: 1,
            tag: 99,
            drops: &kd,
        },
        Value(&vd, 9),
    );
    right.insert(
        Key {
            rank: 3,
            tag: 30,
            drops: &kd,
        },
        Value(&vd, 3),
    );
    left.append(&mut right);
    assert!(right.is_empty());
    let (k, v) = left.get_key_value(&1).unwrap();
    assert_eq!((k.tag, v.1), (10, 9));
    assert_eq!((kd.get(), vd.get()), (1, 1));
    let removed = match left.entry(Key {
        rank: 1,
        tag: 777,
        drops: &kd,
    }) {
        Entry::Occupied(e) => e.remove_entry(),
        Entry::Vacant(_) => panic!("missing"),
    };
    assert_eq!((removed.0.tag, removed.1.1), (10, 9));
    drop(removed);
    let value = match left.entry(Key {
        rank: 2,
        tag: 888,
        drops: &kd,
    }) {
        Entry::Occupied(e) => e.remove(),
        Entry::Vacant(_) => panic!("missing"),
    };
    assert_eq!(value.1, 2);
    drop(value);
    drop(left);
    assert_eq!((kd.get(), vd.get()), (6, 4));
    let set_drops = Cell::new(0);
    let mut destination = BTreeSet::from([Key {
        rank: 1,
        tag: 10,
        drops: &set_drops,
    }]);
    let mut source = BTreeSet::from([
        Key { rank: 1, tag: 20, drops: &set_drops },
        Key { rank: 2, tag: 30, drops: &set_drops },
    ]);
    destination.append(&mut source);
    assert!(source.is_empty());
    assert_eq!(destination.first().unwrap().tag, 10);
    assert_eq!(destination.last().unwrap().tag, 30);
    assert_eq!(set_drops.get(), 1);
    drop(destination);
    assert_eq!(set_drops.get(), 3);
    let a = BTreeSet::from([3u32, 1, 2]);
    let b = BTreeSet::from([2u32, 3, 1]);
    assert_eq!(a, b);
    assert_ne!(a, BTreeSet::from([1u32, 2]));
    let mut it = a.iter();
    assert_eq!(it.next(), Some(&1));
    assert_eq!(it.next_back(), Some(&3));
    assert_eq!(it.len(), 1);
    let mut a = BTreeMap::from([(1u32, 2u32)]);
    let mut empty = BTreeMap::new();
    a.append(&mut empty);
    empty.append(&mut a);
    assert!(a.is_empty());
    assert_eq!(empty[&1], 2);
    println!(
        "required btree {} {} {} {}",
        map.len(),
        hash.finish(),
        kd.get(),
        vd.get()
    );
}
