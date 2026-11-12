use std::collections::{HashMap, HashSet};
struct Key;
struct Value;
fn map<K,V,S>(hasher:S)->HashMap<K,V,S>{HashMap::with_capacity_and_hasher(5,hasher)}
fn set<K,S>(hasher:S)->HashSet<K,S>{HashSet::with_capacity_and_hasher(7,hasher)}
fn main(){
 let map:HashMap<Key,Value,()>=map(());
 let set:HashSet<Key,()>=set(());
 assert!(map.is_empty());assert!(map.capacity()>=5);
 assert!(set.is_empty());assert!(set.capacity()>=7);
 println!("{} {}",map.len(),set.len());
}
