use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::BTreeMap;

#[derive(Clone)]
struct Key<'a> {
    rank: i32,
    identity: i32,
    comparisons: &'a Cell<usize>,
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
        self.comparisons.set(self.comparisons.get() + 1);
        self.rank.cmp(&other.rank)
    }
}
impl Drop for Key<'_> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
#[derive(Clone)]
struct Value<'a> {
    value: i32,
    drops: &'a Cell<usize>,
}
impl Drop for Value<'_> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

fn exercise(mode: i32) {
    let comparisons = Cell::new(0usize);
    let key_drops = Cell::new(0usize);
    let value_drops = Cell::new(0usize);
    let mut map = BTreeMap::new();
    assert!(map.is_empty());
    assert!(map.iter().next().is_none());
    for i in 0..257 {
        let rank = if mode == 0 {
            i
        } else if mode == 1 {
            256 - i
        } else {
            (i * 73) % 257
        };
        let key = Key {
            rank,
            identity: rank,
            comparisons: &comparisons,
            drops: &key_drops,
        };
        assert!(
            map.insert(
                key,
                Value {
                    value: rank * 2,
                    drops: &value_drops
                }
            )
            .is_none()
        );
    }
    // A conservative logarithmic bound, independent of std's node layout.
    assert!(comparisons.get() < 257 * 32);
    comparisons.set(0);
    for rank in 0..258 {
        let query = Key {
            rank,
            identity: -1,
            comparisons: &comparisons,
            drops: &key_drops,
        };
        assert_eq!(map.contains_key(&query), rank < 257);
        if rank < 257 {
            assert_eq!(map.get(&query).unwrap().value, rank * 2);
        } else {
            assert!(map.get_mut(&query).is_none());
        }
    }
    assert!(comparisons.get() < 258 * 64);
    let replacement = Key {
        rank: 42,
        identity: -42,
        comparisons: &comparisons,
        drops: &key_drops,
    };
    let old = map
        .insert(
            replacement,
            Value {
                value: 999,
                drops: &value_drops,
            },
        )
        .unwrap();
    assert_eq!(old.value, 84);
    drop(old);
    let query = Key {
        rank: 42,
        identity: -1,
        comparisons: &comparisons,
        drops: &key_drops,
    };
    let found = map.get_mut(&query).unwrap();
    found.value += 1;
    let mut count = 0;
    for (key, value) in map.iter() {
        assert_eq!(key.rank, count);
        assert_eq!(key.identity, count);
        assert_eq!(value.value, if count == 42 { 1000 } else { count * 2 });
        count += 1;
    }
    assert_eq!(count, 257);
    let mut copy = map.clone();
    let found = copy.get_mut(&query).unwrap();
    found.value = 7;
    assert_eq!(map.get(&query).unwrap().value, 1000);
    assert_eq!(copy.get(&query).unwrap().value, 7);
    let mut count = 0;
    for (key, value) in copy {
        assert_eq!(key.rank, count);
        assert_eq!(value.value, if count == 42 { 7 } else { count * 2 });
        count += 1;
    }
    assert_eq!(count, 257);
    let mut owned = map.into_iter();
    let first = owned.next().unwrap();
    assert_eq!(first.0.rank, 0);
    drop(first);
    drop(owned);
    drop(query);
    assert_eq!(key_drops.get(), 257 * 2 + 258 + 2);
    assert_eq!(value_drops.get(), 257 * 2 + 1);
    println!(
        "mode {} ordered {} drops {} {}",
        mode,
        count,
        key_drops.get(),
        value_drops.get()
    );
}

fn borrowed<'a>(key: &'a i32) {
    let mut map = BTreeMap::new();
    map.insert(key, String::from("borrowed"));
    assert_eq!(map.get(key).unwrap(), "borrowed");
    assert_eq!(map.get(&key).unwrap(), "borrowed");
}
fn main() {
    let empty: BTreeMap<i32, String> = Default::default();
    assert!(empty.into_iter().next().is_none());
    let mut single: BTreeMap<i32, String> = const { BTreeMap::new() };
    single.insert(9, String::from("nine"));
    assert_eq!(single.into_iter().next(), Some((9, String::from("nine"))));
    let mut strings = BTreeMap::new();
    strings.insert(String::from("hello"), String::from("world"));
    assert!(strings.contains_key("hello"));
    strings.get_mut("hello").unwrap().push('!');
    assert_eq!(strings.get("hello").unwrap(), "world!");
    let drops = Cell::new(0usize);
    {
        let mut ordinary = BTreeMap::new();
        for rank in 0..3 {
            ordinary.insert(
                rank,
                Value {
                    value: rank,
                    drops: &drops,
                },
            );
        }
        let mut borrowed = ordinary.iter();
        assert_eq!(*borrowed.next().unwrap().0, 0);
        drop(borrowed);
        assert_eq!(drops.get(), 0);
    }
    assert_eq!(drops.get(), 3);
    let local = 17;
    borrowed(&local);
    exercise(0);
    exercise(1);
    exercise(2);
}
