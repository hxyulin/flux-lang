// Library methods and constant forms that keel f503b726 uses:
// `BTreeMap::into_keys`, `HashSet::drain`, `HashMap::drain`,
// `<[[T; N]]>::as_flattened`, and pass lists built from references to
// named constants inside a constant slice.
use std::collections::{BTreeMap, HashMap, HashSet};

const ROUND: [&str; 2] = ["fold", "merge"];
const PARTS: &[&[&str]] = &[&["start"], &ROUND, &["end"], &ROUND];

const fn total_len(parts: &[&[&str]]) -> usize {
    let mut total = 0;
    let mut i = 0;
    while i < parts.len() {
        total += parts[i].len();
        i += 1;
    }
    total
}

const fn concat<const N: usize>(parts: &[&[&'static str]]) -> [&'static str; N] {
    let mut passes = [""; N];
    let mut next = 0;
    let mut i = 0;
    while i < parts.len() {
        let mut j = 0;
        while j < parts[i].len() {
            passes[next] = parts[i][j];
            next += 1;
            j += 1;
        }
        i += 1;
    }
    assert!(next == N);
    passes
}

const PIPELINE: &[&str] = &concat::<{ total_len(PARTS) }>(PARTS);
const FIRST_ROUND: usize = PARTS[1].len();

fn main() {
    println!("{:?} {} {}", PIPELINE, PIPELINE.len(), FIRST_ROUND);

    let mut frames = BTreeMap::new();
    for (k, v) in [(30, "c"), (10, "a"), (20, "b")] {
        frames.insert(k, v);
    }
    let keys: Vec<i32> = frames.clone().into_keys().collect();
    let back: Vec<i32> = frames.into_keys().rev().collect();
    println!("{keys:?} {back:?}");

    let mut addressed: HashSet<String> = ["f", "g", "h"].iter().map(|s| s.to_string()).collect();
    let capacity = addressed.capacity();
    let mut blocked: Vec<String> = Vec::new();
    blocked.extend(addressed.drain());
    blocked.sort();
    println!("{blocked:?} {} {}", addressed.len(), addressed.capacity() >= capacity);
    addressed.insert("again".to_string());
    println!("{:?}", addressed);

    let mut map: HashMap<u32, u32> = (0..5).map(|i| (i, i * i)).collect();
    let drained = map.drain();
    println!("{}", drained.len());
    let mut pairs: Vec<(u32, u32)> = drained.collect();
    pairs.sort();
    println!("{pairs:?} {}", map.is_empty());
    // A drain dropped early still empties the map.
    map.insert(7, 49);
    map.insert(8, 64);
    let first = map.drain().next();
    println!("{} {}", first.is_some(), map.len());

    let pages: Vec<[u8; 4]> = vec![[1, 2, 3, 4], [5, 6, 7, 8]];
    let flat = pages.as_flattened();
    println!("{flat:?}");
    let mut grid = [[0u16; 3]; 2];
    for (i, cell) in grid.as_flattened_mut().iter_mut().enumerate() {
        *cell = i as u16 * 10;
    }
    println!("{grid:?} {}", grid[..1].as_flattened().len());
}
