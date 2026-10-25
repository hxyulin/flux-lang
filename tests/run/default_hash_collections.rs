use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, DefaultHasher, Hash, Hasher, RandomState};

fn main() {
    let mut digest = 0u64;
    for len in 0..65usize {
        let bytes: Vec<u8> = (0..len).map(|i| (i * 37) as u8).collect();
        let mut whole = DefaultHasher::new(); whole.write(&bytes);
        for split in 0..=len {
            let mut chunked = DefaultHasher::default();
            chunked.write(&bytes[..split]); chunked.write(&bytes[split..]);
            assert_eq!(chunked.finish(), whole.finish());
            chunked.write_u64(u64::MAX);
            digest = digest.wrapping_add(chunked.finish());
        }
    }
    let state = RandomState::new(); let copy = state.clone();
    let mut first = state.build_hasher(); let mut second = copy.build_hasher();
    "é中".hash(&mut first); "é中".hash(&mut second);
    assert_eq!(first.finish(), second.finish());
    let mut map: HashMap<String, usize> = HashMap::with_capacity(300);
    for i in 0..257 { assert!(map.insert(format!("key{i}"), i).is_none()); }
    assert_eq!(map.get("key42"), Some(&42));
    let clone = map.clone(); assert_eq!(clone, map);
    let sum: usize = map.values().copied().sum();
    assert_eq!(sum, 32896);
    for i in 0..257 { assert_eq!(map.remove(format!("key{i}").as_str()), Some(i)); }
    assert!(map.is_empty());
    let mut set: HashSet<usize> = HashSet::new();
    for i in 0..300 { assert!(set.insert(i)); assert!(!set.insert(i)); }
    assert_eq!(set.len(), 300); assert!(set.contains(&42));
    let mut capacity = HashSet::<usize>::with_capacity(19); capacity.insert(7);
    let collected: HashMap<usize, usize> = [(1, 3), (2, 5)].into_iter().collect();
    assert_eq!(collected[&2], 5);
    println!("default hashes {digest} map {sum} set {}", set.len());
}
