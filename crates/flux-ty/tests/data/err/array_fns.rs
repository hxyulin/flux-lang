// Arrays' functions and traits in `core`, misused: a length nothing
// says, `Default` past 32 elements, elements without the trait, and a
// length that differs.

use core::array;

struct S;

fn need_clone<T: Clone>(_t: T) {}

fn need_default<T: Default>() {}

fn uninferred() {
    let a = array::from_fn(|i| i);
}

fn main() {
    need_clone([S]);
    need_default::<[i32; 33]>();
    let c = [S, S] == [S, S];
    let d: [u8; 3] = [1, 2].map(|x: u8| x);
}
