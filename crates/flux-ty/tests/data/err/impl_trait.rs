// Opaque types and `impl Trait` parameters used wrongly.

fn wrong_item() -> impl Iterator<Item = u64> {
    0..5u32
}

fn not_iterator() -> impl Iterator<Item = u32> {
    5u32
}

fn branches(c: bool) -> impl Copy {
    if c { 1u8 } else { 2u16 }
}

fn returns(c: bool) -> impl Copy {
    if c {
        return 1u8;
    }
    2u16
}

fn cycle() -> impl Copy {
    (cycle(),)
}

fn number() -> impl Copy {
    5u32
}

fn takes(x: impl Copy) -> u32 {
    let _ = x;
    1
}

fn bounded<T: Copy>(t: T, f: impl Fn(T) -> u32) -> u32 {
    f(t)
}

fn not_copy() -> impl Fn() -> u32 {
    || 1
}

fn main() {
    let a: u32 = number();
    let b = number() + 1;
    takes::<u8>(1);
    bounded::<u8, u8>(1, |x| x as u32);
    let f = not_copy();
    f.clone();
    let _ = (a, b);
}
