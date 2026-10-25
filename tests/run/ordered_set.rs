use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::rc::Rc;

struct Key { id: u32, label: String, drops: Rc<Cell<usize>> }
impl PartialEq for Key { fn eq(&self, other: &Self) -> bool { self.id == other.id } }
impl Eq for Key {}
impl Ord for Key { fn cmp(&self, other: &Self) -> Ordering { self.id.cmp(&other.id) } }
impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}
impl Drop for Key { fn drop(&mut self) { self.drops.set(self.drops.get() + 1); } }

fn main() {
    let mut set = BTreeSet::from([7, 2, 11, 2]);
    assert_eq!(set.len(), 3);
    assert!(!set.insert(7));
    for value in (0..257).rev() { set.insert(value); }
    assert_eq!(set.len(), 257);
    assert_eq!(set.iter().copied().sum::<i32>(), 32896);
    let cloned = set.clone();
    assert!(cloned.contains(&256));
    assert_eq!(cloned.into_iter().sum::<i32>(), 32896);
    let names: BTreeSet<String> = ["Ω", "alpha", "中", "alpha"].into_iter().map(String::from).collect();
    assert!(names.contains("中"));
    println!("set {:?} {}", names, set.len());

    let drops = Rc::new(Cell::new(0));
    let mut owned = BTreeSet::new();
    for (id, label) in [(2, "second"), (1, "first"), (1, "duplicate")] {
        owned.insert(Key { id, label: String::from(label), drops: drops.clone() });
    }
    assert_eq!(drops.get(), 1);
    assert_eq!(owned.iter().next().unwrap().label, "first");
    let mut iter = owned.into_iter();
    drop(iter.next().unwrap());
    assert_eq!(drops.get(), 2);
    drop(iter);
    assert_eq!(drops.get(), 3);
    println!("owned {}", drops.get());
}
