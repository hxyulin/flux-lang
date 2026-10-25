use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

#[derive(Default)]
struct Collision;
impl Hasher for Collision {
    fn write(&mut self, _: &[u8]) {}
    fn finish(&self) -> u64 { 7 }
}
type Map<K, V> = HashMap<K, V, BuildHasherDefault<Collision>>;
type Set<K> = HashSet<K, BuildHasherDefault<Collision>>;
type Entries<K, V> = Vec<(K, V)>;
type Answer<T> = Result<T, u64>;
type Inferred<T> = Vec<T>;
struct Record<A, B> { left: A, right: B }
type Named<T> = Record<T, u16>;

fn main() {
    let mut map = Map::default();
    map.insert(3u64, 17u64);
    map.insert(11, 29);
    let mut set = Set::default();
    set.insert(String::from("alpha"));
    assert!(set.contains("alpha"));
    let mut entries = Entries::new();
    entries.push((7u8, String::from("item")));
    let answer = Answer::Ok(31u32);
    let error: Answer<u32> = Answer::Err(41);
    let mut inferred = Inferred::new();
    inferred.push(3u8);
    let named = Named { left: 19u8, right: 23 };
    println!("aliases {} {} {:?}", map[&3], set.len(), entries);
    println!("answers {:?} {:?} {:?} {} {}", answer, error, inferred, named.left, named.right);
}
