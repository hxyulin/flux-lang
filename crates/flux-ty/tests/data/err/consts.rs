// Constants calling non-`const` functions, and constants of the wrong type.

fn double(x: i32) -> i32 {
    x * 2
}

const fn triple(x: i32) -> i32 {
    x * 3
}

struct S {
    value: i32,
}

impl S {
    const fn make(value: i32) -> S {
        S { value }
    }

    fn new() -> S {
        S { value: 0 }
    }

    fn get(&self) -> i32 {
        self.value
    }

    const fn peek(&self) -> i32 {
        self.value
    }

    const BAD: i32 = double(1);
    const GOOD: i32 = triple(1) + S::make(2).peek();
}

const A: i32 = double(1);
const B: i32 = triple(double(2));
const C: i32 = S::new().value;
const D: i32 = S::make(1).get();
const E: i32 = 3i32.max(4);
const F: i32 = { let s = S::make(1); s.get() + double(2) };

const fn calls_plain(x: i32) -> i32 {
    double(x) + S::new().get() + triple(x)
}

const fn calls_core(x: i32) -> i32 {
    x.min(10)
}

const WRONG: i32 = true;
const WRONG_TUPLE: (i32, bool) = (1, 2);
const WRONG_STRUCT: S = 5;
const NARROW: u8 = A;

impl S {
    const MISMATCH: bool = 1;
}

fn main() {
    let _all = A + B + C + D + E + F + S::BAD + S::GOOD + calls_plain(1) + calls_core(2);
    let _wrong = (WRONG, WRONG_TUPLE, WRONG_STRUCT.value, NARROW, S::MISMATCH);
}
