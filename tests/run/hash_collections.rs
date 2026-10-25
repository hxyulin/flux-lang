use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::cell::Cell;

#[derive(Default)]
struct Collision;
impl Hasher for Collision {
    fn write(&mut self, _: &[u8]) {}
    fn finish(&self) -> u64 { 7 }
}
type Map<K, V> = HashMap<K, V, BuildHasherDefault<Collision>>;
type Set<K> = HashSet<K, BuildHasherDefault<Collision>>;

struct Key { id: usize, label: usize }
impl PartialEq for Key { fn eq(&self, other: &Self) -> bool { self.id == other.id } }
impl Eq for Key {}
impl Hash for Key { fn hash<H: Hasher>(&self, state: &mut H) { self.id.hash(state); } }
struct Owner<'a>(&'a Cell<usize>, usize);
impl Drop for Owner<'_> { fn drop(&mut self) { self.0.set(self.0.get() + 1); } }

fn main() {
    let mut map = Map::default();
    for key in 0..1024 { assert_eq!(map.insert(key, key * 3), None); }
    for key in (0..1024).step_by(2) { assert_eq!(map.remove(&key), Some(key * 3)); }
    for key in 1024..1600 { map.insert(key, key * 3); }
    for key in 0..1024 { assert_eq!(map.contains_key(&key), key % 2 == 1); }
    for key in 1024..1600 { assert_eq!(map[&key], key * 3); }
    let mut factory_calls = 0;
    *map.entry(1).or_insert_with(|| { factory_calls += 1; 99 }) += 2;
    map.entry(0).and_modify(|value| *value += 1).or_insert_with(|| { factory_calls += 1; 123 });
    assert_eq!(factory_calls, 1);
    assert_eq!(map[&1], 5);
    for value in map.values_mut() { *value += 1; }
    let cloned = map.clone();
    assert_eq!(cloned, map);
    map.retain(|key, value| { *value += 7; *key % 3 == 0 });
    let sum: usize = map.iter().map(|(key, value)| key + value).sum();
    assert!(cloned.len() > map.len());
    assert_eq!(cloned.clone().into_keys().sum::<usize>(), cloned.keys().copied().sum::<usize>());
    assert_eq!(cloned.clone().into_values().sum::<usize>(), cloned.values().copied().sum::<usize>());
    assert_eq!(map.iter().len(), map.len());
    let mut owned = map.into_iter();
    assert_eq!(owned.len(), owned.size_hint().0);
    owned.next();
    drop(owned);

    let mut borrowed: Map<String, usize> = Map::default();
    borrowed.insert(String::from("alpha"), 3);
    *borrowed.get_mut("alpha").unwrap() += 4;
    assert_eq!(borrowed.get_key_value("alpha").unwrap().0.as_str(), "alpha");
    assert_eq!(borrowed.remove_entry("alpha"), Some((String::from("alpha"), 7)));
    assert!(borrowed.is_empty());

    let mut keys = Map::default();
    keys.insert(Key { id: 1, label: 10 }, 20);
    assert_eq!(keys.insert(Key { id: 1, label: 30 }, 40), Some(20));
    let key = Key { id: 1, label: 0 };
    assert_eq!(keys.get_key_value(&key).unwrap().0.label, 10);
    match keys.entry(key) {
        std::collections::hash_map::Entry::Occupied(mut entry) => {
            assert_eq!(entry.insert(50), 40);
            assert_eq!(entry.remove_entry().0.label, 10);
        }
        _ => panic!("missing key"),
    }

    let mut set: Set<usize> = (0..512).collect();
    for value in 0..512 { assert!(!set.insert(value)); }
    for value in (0..512).step_by(2) { assert!(set.remove(&value)); }
    let other = set.clone();
    assert!(set.is_subset(&other));
    assert!(!set.is_disjoint(&other));
    set.retain(|value| value % 3 == 1);
    let set_sum: usize = set.into_iter().sum();
    let mut units: Set<()> = Set::default();
    assert!(units.insert(())); assert!(!units.insert(())); assert!(units.take(&()).is_some());

    let drops = Cell::new(0);
    {
        let mut owners = Map::default();
        for key in 0..100 { owners.insert(key, Owner(&drops, key)); }
        drop(owners.remove(&4));
        assert_eq!(drops.get(), 1);
        owners.retain(|_, owner| owner.1 % 2 == 0);
        assert_eq!(drops.get(), 51);
        let mut iter = owners.into_iter();
        drop(iter.next());
        assert_eq!(drops.get(), 52);
    }
    assert_eq!(drops.get(), 100);
    let mut reused = Map::<usize, usize>::default();
    for cycle in 0..50 {
        for key in 0..64 { reused.insert(key, cycle); }
        for key in 0..64 { assert_eq!(reused.remove(&key), Some(cycle)); }
    }
    reused.clear();
    println!("maps {sum} set {set_sum} drops {}", drops.get());
}
