use std::collections::BTreeSet;

fn main() {
    let empty = BTreeSet::<u32>::new();
    let mut set: BTreeSet<_> = (0..512u32).rev().collect();
    let outside = BTreeSet::from([512, 1024]);
    let overlap = BTreeSet::from([511, 1024]);
    println!("{} {} {}", empty.first().is_none(), empty.is_disjoint(&empty), set.first().unwrap());
    println!("{} {} {}", set.is_disjoint(&empty), empty.is_disjoint(&set), set.is_disjoint(&set));
    println!("{} {} {} {}", set.is_disjoint(&outside), outside.is_disjoint(&set), set.is_disjoint(&overlap), overlap.is_disjoint(&set));
    assert_eq!(set.pop_first(), Some(0));
    println!("{} {}", set.first().unwrap(), set.len());
}
