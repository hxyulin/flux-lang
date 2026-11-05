// `RandomState`: clones hash alike, new builders differ from each other,
// on the main thread and on spawned ones, and maps built with them keep
// working after many have been made.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, Hash, Hasher, RandomState};

fn hash_with<T: Hash>(state: &RandomState, value: &T) -> u64 {
    let mut hasher = state.build_hasher();
    value.hash(&mut hasher);
    hasher.finish()
}

fn distinct_builders() -> (bool, bool, usize) {
    let first = RandomState::new();
    let copy = first.clone();
    let same = hash_with(&first, &"key") == hash_with(&copy, &"key");
    let states: Vec<RandomState> = (0..64).map(|_| RandomState::new()).collect();
    let hashes: HashSet<u64> = states.iter().map(|s| hash_with(s, &42u32)).collect();
    let repeat = hash_with(&states[3], &42u32) == hash_with(&states[3], &42u32);
    (same, repeat, hashes.len())
}

fn main() {
    let (same, repeat, distinct) = distinct_builders();
    println!("main {same} {repeat} {}", distinct == 64);

    let mut total = 0usize;
    for i in 0..2000u32 {
        let mut map: HashMap<u32, u32> = HashMap::new();
        map.insert(i, i * 2);
        map.insert(i + 1, i * 3);
        total += map[&i] as usize + map.len();
    }
    println!("maps {total}");

    let handles: Vec<_> = (0..4)
        .map(|t| {
            std::thread::spawn(move || {
                let (same, repeat, distinct) = distinct_builders();
                let mut map: HashMap<String, usize> = HashMap::new();
                for i in 0..100 {
                    map.insert(format!("{t}-{i}"), i);
                }
                (same && repeat && distinct == 64, map.len(), map["3-50".replace('3', &t.to_string()).as_str()])
            })
        })
        .collect();
    for handle in handles {
        let (ok, len, value) = handle.join().unwrap();
        println!("thread {ok} {len} {value}");
    }
    let set: HashSet<&str> = ["a", "b", "a"].into_iter().collect();
    std::process::exit(set.len() as i32 + 40);
}
