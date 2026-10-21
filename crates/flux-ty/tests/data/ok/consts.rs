// Constants: free and associated, of every supported type, computed with
// operators, other constants, `const fn`s and `core`'s integer methods.

const ANSWER: i32 = 42;
const NEGATIVE: i64 = -(ANSWER as i64) * 2;
const BYTE: u8 = 0xff;
const FLAG: bool = ANSWER > 40 && !(BYTE == 0);
const PAIR: (i32, bool) = (ANSWER, FLAG);
const NESTED: ((u8, u16), ()) = ((1, 2), ());
const ORIGIN: Point = Point { x: 0, y: ZERO };
const ZERO: i32 = 0;
const SHIFTED: u32 = 1 << 31;
const MASKED: u64 = (u64::MAX >> 4) & 0xf0f0;
const LIMITS: (i8, u128, isize, usize) = (i8::MIN, u128::MAX, isize::MAX, usize::BITS as usize);
const SQUARE: i32 = square(ANSWER);
const CHOSEN: i32 = choose(FLAG, 1, 2);
const LOOPED: u32 = sum_to(10);
const METHODS: u32 = 0b1011u8.count_ones() + 16u16.trailing_zeros() + 1u32.leading_zeros();
const SIGNED: i32 = (-7i32).abs() + (-7i32).signum() + (-7i32).rem_euclid(3) + 2i32.pow(10);
const WRAPPED: u8 = u8::MAX.wrapping_add(2);
const ROTATED: u16 = 0x8001u16.rotate_left(1);
const DIFF: u32 = 3u32.abs_diff(10);
const BLOCK: i32 = {
    let mut acc = 0;
    let mut i = 0;
    while i < 4 {
        acc += i;
        i += 1;
    }
    if acc > 5 { acc } else { -acc }
};
const CAST: u8 = 300i32 as u8;
const TYPED: Counter = Counter::new(Counter::START);

struct Point {
    x: i32,
    y: i32,
}

struct Counter {
    value: u32,
}

impl Counter {
    const START: u32 = 5;
    const STEP: u32 = Self::START * 2 + LOOPED;
    const ZERO: Counter = Counter { value: 0 };

    const fn new(value: u32) -> Counter {
        Counter { value }
    }

    const fn next(self) -> Counter {
        Counter { value: self.value + Self::STEP }
    }

    const fn get(&self) -> u32 {
        self.value
    }
}

const fn square(x: i32) -> i32 {
    x * x
}

const fn choose(c: bool, a: i32, b: i32) -> i32 {
    if c { a } else { b }
}

const fn sum_to(n: u32) -> u32 {
    let mut total = 0;
    let mut i = 1;
    while i <= n {
        total += i;
        i += 1;
    }
    total
}

const fn uses_consts() -> u32 {
    Counter::START + Counter::ZERO.get() + Counter::new(1).next().value
}

fn main() {
    let p = PAIR.0 + NESTED.0.1 as i32 + ORIGIN.x + ORIGIN.y + SQUARE + CHOSEN + SIGNED + BLOCK;
    let q = NEGATIVE + MASKED as i64 + LIMITS.0 as i64 + (LIMITS.2 >> 60) as i64 + LIMITS.3 as i64;
    let r = SHIFTED + METHODS + WRAPPED as u32 + ROTATED as u32 + DIFF + CAST as u32;
    let s = TYPED.get() + Counter::STEP + uses_consts() + (LIMITS.1 >> 125) as u32 + NESTED.0.0 as u32;
    let _all = (p as i64 + q + (r + s) as i64, PAIR.1, NESTED.1);
}
