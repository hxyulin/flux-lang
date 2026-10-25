use std::sync::atomic::{AtomicUsize, Ordering};

static DROPS: AtomicUsize = AtomicUsize::new(0);
struct Item(u32);
impl Drop for Item {
    fn drop(&mut self) { DROPS.fetch_add(self.0 as usize, Ordering::Relaxed); }
}
struct Owner([Item; 4]);
fn owner() -> Owner { Owner([Item(1), Item(2), Item(3), Item(4)]) }
fn move_owner(value: Owner) -> Owner { value }
fn make() -> [u64; 4096] { let values = [9; 4096]; values }
fn through(value: [u64; 4096]) -> [u64; 4096] { value }
fn mutate(mut value: [u64; 64], other: &[u64; 64]) -> [u64; 64] {
    value[0] += 7;
    value[1] = other[0];
    value
}
fn two(mut first: [u64; 64], second: [u64; 64]) -> [u64; 64] {
    first[0] += 11;
    first[1] = second[0];
    first
}
fn shift(value: &[u64; 64]) -> [u64; 64] {
    let mut output = [0; 64];
    for i in 0..64 { output[i] = value[(i + 63) % 64] + i as u64; }
    output
}
fn main() {
    let source = [13; 64];
    let changed = mutate(source, &source);
    assert_eq!(source[0], 13);
    assert_eq!(changed[0], 20); assert_eq!(changed[1], 13);
    let changed = two(source, source);
    assert_eq!(source[0], 13);
    assert_eq!(changed[0], 24); assert_eq!(changed[1], 13);
    let mut aliased = [0; 64];
    for i in 0..64 { aliased[i] = i as u64; }
    aliased = shift(&aliased);
    assert_eq!(aliased[0], 63); assert_eq!(aliased[1], 1); assert_eq!(aliased[63], 125);
    let call: fn(&[u64; 64]) -> [u64; 64] = shift;
    aliased = call(&aliased);
    assert_eq!(aliased[0], 125); assert_eq!(aliased[1], 64);
    let pointer = &mut aliased as *mut [u64; 64];
    unsafe { *pointer = shift(&*pointer); }
    assert_eq!(aliased[0], 186); assert_eq!(aliased[1], 126);
    let large = make();
    let passed = through(large);
    assert_eq!(large[0], 9); assert_eq!(large[4095], 9); assert_eq!(passed[4095], 9);
    let a = move_owner(move_owner(owner()));
    assert_eq!(DROPS.load(Ordering::Relaxed), 0);
    assert_eq!(a.0[3].0, 4);
    drop(a);
    assert_eq!(DROPS.load(Ordering::Relaxed), 10);
    println!("{} {} {} {}", changed[0], aliased[0], passed[0], DROPS.load(Ordering::Relaxed));
}
