use std::cell::Cell;
struct Owner<'a> { id: u32, drops: &'a Cell<u32> }
impl Drop for Owner<'_> { fn drop(&mut self) { self.drops.set(self.drops.get() + 1); } }
fn target<'a>(values: &'a mut [u32; 2], events: &Cell<u32>, index: usize) -> &'a mut u32 {
    events.set(events.get() * 10 + index as u32 + 2);
    &mut values[index]
}
fn source(events: &Cell<u32>) -> (u32, u32) { events.set(events.get() * 10 + 1); (7, 11) }
fn main() {
    let a; let b;
    (a, b) = (3u32, 5u32);
    assert_eq!((a, b), (3, 5));
    let events = Cell::new(0);
    let mut values = [0, 0];
    (*target(&mut values, &events, 0), *target(&mut values, &events, 1)) = source(&events);
    assert_eq!(events.get(), 123);
    assert_eq!(values, [7, 11]);
    let drops = Cell::new(0);
    {
        let mut first = Owner { id: 1, drops: &drops };
        let mut second = Owner { id: 2, drops: &drops };
        (first, second) = (second, first);
        assert_eq!((first.id, second.id, drops.get()), (2, 1, 0));
        (first, second) = (Owner { id: 3, drops: &drops }, Owner { id: 4, drops: &drops });
        assert_eq!(drops.get(), 2);
        let mut third = Owner { id: 5, drops: &drops };
        ((first, second), third) = ((third, first), second);
        assert_eq!((first.id, second.id, third.id), (5, 3, 4));
    }
    assert_eq!(drops.get(), 5);
    let mut pair = (13u32, 17u32);
    (pair.0, pair.1) = (pair.1, pair.0);
    println!("tuple {:?} {} {}", pair, events.get(), drops.get());
}
