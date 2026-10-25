use std::cell::Cell;
struct Inspect<'a>(&'a Cell<usize>);
impl Drop for Inspect<'_> { fn drop(&mut self) { self.0.set(self.0.get() + 1); } }
fn main() {
    // The containers precede the referents, but their final use precedes
    // referent destruction. Dropping references never reads their referents.
    let mut borrowed = Vec::new();
    let mut nested = Vec::new();
    let iterator;
    let boxed;
    let values = vec![7u32, 11, 29];
    boxed = Box::new(vec![Box::new(&values[0])]);
    assert_eq!(**boxed[0], 7);
    borrowed.push(&values[0]);
    nested.push(vec![vec![&values[1]]]);
    iterator = vec![&values[2]].into_iter();
    assert_eq!(*borrowed[0], 7);
    assert_eq!(*nested[0][0][0], 11);
    assert_eq!(**iterator.as_slice().first().unwrap(), 29);
    let drops = Cell::new(0);
    { let owners = vec![Inspect(&drops), Inspect(&drops)]; assert_eq!(owners.len(), 2); }
    assert_eq!(drops.get(), 2);
    println!("drop regions {}", drops.get());
}
