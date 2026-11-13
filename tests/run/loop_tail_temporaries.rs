use std::cell::Cell;
use std::rc::Rc;
struct Read<'a> { value: &'a u32, dropped: &'a Cell<u32> }
impl Drop for Read<'_> {
    fn drop(&mut self) {
        assert_eq!(*self.value, self.dropped.get() + 1);
        self.dropped.set(self.dropped.get() + 1);
    }
}
fn step<'a>(value: &'a mut u32, dropped: &'a Cell<u32>) -> Result<Read<'a>, ()> {
    assert_eq!(*value, dropped.get());
    *value += 1;
    Ok(Read { value, dropped })
}
struct Stamp(Rc<Cell<u64>>, u64);
impl Drop for Stamp {
    fn drop(&mut self) { self.0.set(self.0.get() * 10 + self.1); }
}
fn main() {
    let mut value = 0;
    let dropped = Cell::new(0);
    while value < 3 {
        match step(&mut value, &dropped) { Ok(_) => {}, Err(()) => break }
    }
    assert_eq!(value, 3);
    assert_eq!(dropped.get(), 3);
    let events = Rc::new(Cell::new(0));
    let mut iterations = 0;
    while iterations < 2 {
        let _local = Stamp(events.clone(), 1);
        iterations += 1;
        match Stamp(events.clone(), 2) { _ => () }
    }
    // Ordinary block tails retain the surrounding statement's temporary
    // scope before 2024; the loop fix must not change that drop order.
    {
        let _local = Stamp(events.clone(), 3);
        match Stamp(events.clone(), 4) { _ => () }
    }
    println!("{} {} {}", value, dropped.get(), events.get());
}
