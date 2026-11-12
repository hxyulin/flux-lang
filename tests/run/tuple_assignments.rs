use std::cell::Cell;
struct Dropped(&'static str);
impl Drop for Dropped {
    fn drop(&mut self) { println!("drop {}", self.0); }
}
fn place<'a>(values: &'a mut [u32; 2], calls: &Cell<u32>, index: usize) -> &'a mut u32 {
    calls.set(calls.get() + 1);
    println!("place {index}");
    &mut values[index]
}
fn values() -> (u32, u32) { println!("rhs"); (7, 11) }
fn main() {
    let (mut a, mut b) = (1, 2);
    (a, b) = (b, a);
    println!("{a} {b}");
    let mut data = [0, 0];
    let calls = Cell::new(0);
    (*place(&mut data, &calls, 0), *place(&mut data, &calls, 1)) = values();
    println!("{:?} {}", data, calls.get());
    let (mut x, mut y) = (String::new(), String::new());
    (x, (_, y)) = (String::from("first"), (String::from("ignored"), String::from("last")));
    println!("{x} {y}");
    let mut kept = Dropped("old");
    (_, kept) = (Dropped("ignored"), Dropped("new"));
    println!("after {}", kept.0);
    let mut uninitialized;
    (_, uninitialized) = (9, 13);
    println!("{uninitialized}");
}
