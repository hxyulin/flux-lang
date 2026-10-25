use std::cell::Cell;
struct Aggregate<'a> { borrowed: &'a mut u32, owned: Vec<u32> }
struct Watch<'a> { value: &'a u32, drops: &'a Cell<u32> }
impl Drop for Watch<'_> { fn drop(&mut self) { self.drops.set(self.drops.get() + *self.value); } }
struct Mixed<'a, 'b> { borrowed: &'a mut u32, watched: Watch<'b> }
fn main() {
    let mut value = 7u32;
    let aggregate = Aggregate { borrowed: &mut value, owned: vec![11u32, 13] };
    assert_eq!(*aggregate.borrowed, 7);
    value = 17;
    assert_eq!(value, 17);
    let mut second = 19u32;
    let tuple = (&mut second, vec![23u32]);
    assert_eq!(*tuple.0, 19);
    second = 29;
    let mut third = 31u32;
    let observed = 37u32;
    let drops = Cell::new(0);
    {
        let mixed = Mixed { borrowed: &mut third, watched: Watch { value: &observed, drops: &drops } };
        assert_eq!(*mixed.borrowed, 31);
        third = 41;
        assert_eq!(third, 41);
    }
    assert_eq!(drops.get(), 37);
    println!("aggregate {} {} {} {}", value, second, third, drops.get());
}
