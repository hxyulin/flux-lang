// Array lengths and repeat counts written as constant expressions, each
// an anonymous constant of type `usize` evaluated while types are
// checked.

const LIMIT: usize = 6;

const fn double(n: usize) -> usize {
    n * 2
}

struct Sieve {
    composite: [bool; LIMIT + 1],
}

struct Buffer;

impl Buffer {
    const CAP: usize = 2;

    fn fresh() -> [u8; Self::CAP * 2] {
        [7; Self::CAP + 2]
    }
}

fn main() {
    let s = Sieve {
        composite: [false; LIMIT + 1],
    };
    let d: [u16; double(3)] = [5; { LIMIT }];
    let b = Buffer::fresh();
    let n = s.composite.len() + d.len() + b.len();
}
