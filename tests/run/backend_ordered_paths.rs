use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound::{Excluded, Included, Unbounded};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

struct Item<'a> { key: usize, drops: &'a Cell<usize> }
impl Drop for Item<'_> { fn drop(&mut self) { self.drops.set(self.drops.get() + 1); } }

fn digest(state: &mut u64, bytes: &[u8]) {
    *state = state.wrapping_mul(1099511628211).wrapping_add(bytes.len() as u64);
    for &byte in bytes { *state = state.wrapping_mul(1099511628211).wrapping_add(byte as u64); }
}

fn main() {
    let mut map = BTreeMap::new();
    for i in (0..257).rev() { map.insert(i, i * 3); }
    let mut visited = Vec::new();
    map.retain(|key, value| { visited.push(*key); *value += 1; key % 3 != 0 });
    assert_eq!(visited, (0..257).collect::<Vec<_>>());
    assert_eq!(map.len(), 171);
    assert_eq!(map.first_key_value(), Some((&1, &4)));
    assert_eq!(map.last_key_value(), Some((&256, &769)));
    for lower in 0..22 {
        for upper in lower..22 {
            let expected: Vec<_> = map.iter().filter(|(key, _)| **key >= lower && **key < upper).map(|(key, value)| (*key, *value)).collect();
            let actual: Vec<_> = map.range(lower..upper).map(|(key, value)| (*key, *value)).collect();
            assert_eq!(actual, expected);
            let reverse: Vec<_> = map.range(lower..upper).rev().map(|(key, value)| (*key, *value)).collect();
            assert_eq!(reverse, expected.iter().rev().copied().collect::<Vec<_>>());
        }
    }
    let mut range = map.range((Included(7), Included(19)));
    assert_eq!(range.next(), Some((&7, &22)));
    assert_eq!(range.next_back(), Some((&19, &58)));
    let rest: Vec<_> = range.clone().map(|(key, _)| *key).collect();
    assert_eq!(rest, vec![8, 10, 11, 13, 14, 16, 17]);
    assert_eq!(range.rev().map(|(key, _)| *key).collect::<Vec<_>>(), vec![17,16,14,13,11,10,8]);
    let mut words = BTreeMap::new();
    for word in ["a", "b", "c", "d"] { words.insert(String::from(word), word.len()); }
    assert_eq!(words.range::<str,_>((Included("b"), Excluded("d"))).map(|(key, _)| key.as_str()).collect::<Vec<_>>(), vec!["b", "c"]);
    assert_eq!(words.range::<str,_>((Unbounded, Included("a"))).count(), 1);
    let drops = Cell::new(0);
    {
        let mut owned = BTreeMap::new();
        for i in 0..64 { owned.insert(i, Item { key: i, drops: &drops }); }
        owned.retain(|key, _| key % 2 == 0);
        assert_eq!(drops.get(), 32);
        let mut values = owned.into_values();
        assert_eq!(values.len(), 32);
        assert_eq!(values.next().unwrap().key, 0);
        assert_eq!(values.next_back().unwrap().key, 62);
        assert_eq!(values.len(), 30);
    }
    assert_eq!(drops.get(), 64);
    let mut set: BTreeSet<_> = ["c", "a", "b", "d"].into_iter().map(String::from).collect();
    assert!(set.remove("c"));
    assert!(!set.remove("missing"));
    set.retain(|value| value.as_str() != "b");
    assert_eq!(set.pop_first(), Some(String::from("a")));
    assert_eq!(set.pop_first(), Some(String::from("d")));
    assert_eq!(set.pop_first(), None);
    set.insert(String::from("again"));
    set.clear(); assert!(set.is_empty());
    map.clear(); assert!(map.is_empty());
    map.insert(3,4); assert_eq!(map.pop_first(), Some((3,4)));
    for spelling in ["", ".", "..", "/", "/a/./", "a/.", "a//b/..", "a/..", "a//b/.", "a//b//", "a/.hidden", "foo.", "foo.tar.gz", "foo/../.", "a//b/c"] {
        let path = Path::new(spelling);
        println!("{:?} {:?} {:?} {:?} {:?}", spelling, path.file_name(), path.with_file_name("new"), path.with_extension("rs"), path.with_extension(""));
    }
    for bytes in [vec![b'/', 0xff, b'.', b'x'], vec![b'a',b'/',0xfe,b'.',b'.'], vec![0xff]] {
        let path = PathBuf::from(std::ffi::OsString::from_vec(bytes));
        let replaced = path.with_extension(std::ffi::OsStr::from_bytes(&[0xfd]));
        println!("{:?} {:?}", path.file_name().unwrap().as_bytes(), replaced.as_os_str().as_bytes());
    }
    let mut state = 0u64;
    for encoded in 0..15625usize {
        let mut number = encoded;
        let mut bytes = Vec::new();
        for _ in 0..6 {
            bytes.push([b'a', b'.', b'/', 0xff, b'x'][number % 5]);
            number /= 5;
        }
        let path = PathBuf::from(std::ffi::OsString::from_vec(bytes));
        match path.file_name() {
            Some(name) => { digest(&mut state, &[1]); digest(&mut state, name.as_bytes()); },
            None => digest(&mut state, &[0]),
        }
        for replacement in ["", "new", "../next", "/abs"] {
            digest(&mut state, path.with_file_name(replacement).as_os_str().as_bytes());
        }
        for extension in ["", "rs", ".", ".tail", "a.b"] {
            digest(&mut state, path.with_extension(extension).as_os_str().as_bytes());
        }
    }
    println!("ordered paths {} {}", drops.get(), state);
}
