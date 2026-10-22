// Arrays are not iterable by value yet: rustc's `core` makes them so
// with const generics. References to them and slices are.

fn main() {
    let a = [1, 2, 3];
    for x in a {}
    for x in &a {}
    let s: &[i32] = &a;
    for x in s {}
}
