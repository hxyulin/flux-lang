// Tuples and arrays compared where their elements cannot be, or with
// other types: `core`'s impls for them are what fails, as in rustc.

struct S;

#[derive(PartialEq)]
struct OnlyEq;

fn main() {
    let _ = (1, S) == (1, S);
    let _ = (1, OnlyEq) < (1, OnlyEq);
    let _ = (1, 2) == (1, 2, 3);
    let _ = (1u8, true) == (1u16, true);
    let _ = [OnlyEq] < [OnlyEq];
    let _ = (1, 2) + (3, 4);
}
