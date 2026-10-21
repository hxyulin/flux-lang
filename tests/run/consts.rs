//! `const` items: free and associated constants of integer, `bool`, unit,
//! tuple and struct types; constants defined from other constants (in any
//! order); `const fn`s evaluated at compile time in const initializers and
//! called at run time on parameters; constants as loop bounds, as method
//! receivers (`LIMIT.pow(2)`) and as fresh struct values at every use.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

const LIMIT: u32 = 12;
const SQUARE: u32 = LIMIT * LIMIT;
// Defined before the constant it refers to.
const TOTAL: u64 = TRIANGLE as u64 + 1000;
const TRIANGLE: u32 = triangle(LIMIT);
const NEG: i32 = -(LIMIT as i32) * 3;
const FLAG: bool = SQUARE > 100 && !(NEG > 0);
const OFF: bool = !FLAG || LIMIT == 0;
const MASK: u8 = (1 << 5) - 1 | 0x80;
const SHIFTED: i64 = (NEG as i64) << 40 >> 3;
const HUGE: u128 = (1 << 100) + TOTAL as u128;
const SMALL: i128 = -(HUGE as i128) / 7;
const UNIT: () = ();
const BLOCK: i32 = {
    let a = 7;
    let mut b = a * a;
    b -= 9;
    b + NEG
};

const PAIR: (i32, bool) = (NEG + 1, FLAG);
const NESTED: ((u8, i16), (u64, ())) = ((MASK, -300), (TOTAL, UNIT));
const FROM_PAIR: i32 = PAIR.0 * 2;
const FROM_NESTED: i16 = (NESTED.0).1 + 1;

// `const fn`s: loops, recursion, early returns, calls to other `const fn`s
// and to `core`'s integer methods.
const fn triangle(n: u32) -> u32 {
    let mut i = 0;
    let mut sum = 0;
    while i <= n {
        sum += i;
        i += 1;
    }
    sum
}

const fn fib(n: u32) -> u64 {
    let mut a = 0;
    let mut b = 1;
    let mut i = 0;
    while i < n {
        let t = a + b;
        a = b;
        b = t;
        i += 1;
    }
    a
}

const fn fib_rec(n: u32) -> u64 {
    if n < 2 {
        n as u64
    } else {
        fib_rec(n - 1) + fib_rec(n - 2)
    }
}

const fn gcd(a: u64, b: u64) -> u64 {
    let mut a = a;
    let mut b = b;
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

const fn isqrt(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let mut lo = 1;
    let mut hi = n;
    loop {
        if lo > hi {
            break hi;
        }
        let mid = lo + (hi - lo) / 2;
        if mid <= n / mid {
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
}

const fn collatz_steps(n: u64) -> u32 {
    let mut n = n;
    let mut steps = 0;
    while n != 1 {
        n = if n % 2 == 0 { n / 2 } else { 3 * n + 1 };
        steps += 1;
    }
    steps
}

const fn bits(x: u64) -> u32 {
    x.count_ones() * 100 + x.leading_zeros()
}

const fn classify(x: i32) -> (bool, i32) {
    (x.is_negative(), x.signum() * x.abs())
}

const fn past_limit(x: u32) -> u32 {
    x + LIMIT * Point::SCALE as u32
}

const FIB_30: u64 = fib(30);
const PAST: u32 = past_limit(SQUARE);
const FIB_REC: u64 = fib_rec(15);
const GCD: u64 = gcd(FIB_30, fib(20) * 6);
const ROOT: u64 = isqrt(FIB_30 * FIB_30 + 5);
const STEPS: u32 = collatz_steps(27);
const BITS: u32 = bits(FIB_30);
const CLASS: (bool, i32) = classify(NEG);
const POW: u32 = LIMIT.pow(2) + 2u32.pow(LIMIT);
const ROT: u16 = 0x8001u16.rotate_left(LIMIT);

// Struct constants. `Point` is not `Copy`: each use of a constant is a
// fresh value, so they can be used any number of times.
struct Point {
    x: i32,
    y: i32,
}

struct Rgb(u8, u8, u8);

struct Marker;

struct Segment {
    from: Point,
    to: Point,
    color: Rgb,
}

const ORIGIN: Point = Point { x: 0, y: 0 };
const ONE: i32 = 1;
const DIAG: Point = Point { x: ONE, y: ONE };
const FAR: Point = Point::new(NEG, BLOCK);
const MOVED: Point = Point { x: 9, ..FAR };
const RED: Rgb = Rgb(255, 0, 0);
const MARK: Marker = Marker;
const SEGMENT: Segment = Segment {
    from: ORIGIN,
    to: Point::new(3, 4).scaled(Point::SCALE),
    color: RED,
};

impl Point {
    const SCALE: i32 = 5;
    const UNIT_X: Point = Point::new(1, 0);
    const SCALED_DIAG: Point = Self::new(Self::SCALE, Self::SCALE * ONE);
    const LIMIT_AREA: i32 = Self::SCALE * LIMIT as i32;

    const fn new(x: i32, y: i32) -> Point {
        Point { x, y }
    }

    const fn scaled(self, k: i32) -> Self {
        Self::new(self.x * k, self.y * k)
    }

    const fn len2(&self) -> i32 {
        self.x * self.x + self.y * self.y
    }

    fn describe(&self, name: *const i8) {
        unsafe {
            printf(c"%s = (%d, %d)\n".as_ptr(), name, self.x, self.y);
        }
    }

    fn area_ratio(&self) -> i32 {
        self.x * self.y / Self::LIMIT_AREA
    }
}

impl Rgb {
    const WHITE: Rgb = Rgb(Self::MAX, Self::MAX, Self::MAX);
    const MAX: u8 = u8::MAX;

    const fn sum(&self) -> u32 {
        self.0 as u32 + self.1 as u32 + self.2 as u32
    }
}

impl Marker {
    const ID: u64 = fib(12) * 1000 + Rgb::MAX as u64;

    fn id(self) -> u64 {
        Self::ID
    }
}

fn take(p: Point) -> i32 {
    p.x * 10 + p.y
}

fn show_point(p: &Point) -> i32 {
    p.x + p.y
}

fn line_u(label: *const i8, v: u64) {
    unsafe {
        printf(c"%s = %llu\n".as_ptr(), label, v);
    }
}

fn line_i(label: *const i8, v: i64) {
    unsafe {
        printf(c"%s = %lld\n".as_ptr(), label, v);
    }
}

fn line_b(label: *const i8, v: bool) {
    unsafe {
        printf(c"%s = %d\n".as_ptr(), label, v as i32);
    }
}

fn line_x(label: *const i8, v: u128) {
    unsafe {
        printf(
            c"%s = 0x%016llx%016llx\n".as_ptr(),
            label,
            (v >> 64) as u64,
            v as u64,
        );
    }
}

// The same `const fn`s at run time, on parameters.
fn run_time(n: u32, a: u64, b: u64, x: i32) {
    line_u(c"triangle(n)".as_ptr(), triangle(n) as u64);
    line_u(c"fib(n)".as_ptr(), fib(n));
    line_u(c"fib_rec(n)".as_ptr(), fib_rec(n));
    line_u(c"gcd(a, b)".as_ptr(), gcd(a, b));
    line_u(c"isqrt(a)".as_ptr(), isqrt(a));
    line_u(c"collatz_steps(b)".as_ptr(), collatz_steps(b) as u64);
    line_u(c"bits(a)".as_ptr(), bits(a) as u64);
    line_u(c"past_limit(n)".as_ptr(), past_limit(n) as u64);
    let (neg, abs) = classify(x);
    line_b(c"classify(x).0".as_ptr(), neg);
    line_i(c"classify(x).1".as_ptr(), abs as i64);
    let p = Point::new(x, n as i32).scaled(Point::SCALE);
    line_i(c"p.len2()".as_ptr(), p.len2() as i64);
}

fn main() {
    line_u(c"LIMIT".as_ptr(), LIMIT as u64);
    line_u(c"SQUARE".as_ptr(), SQUARE as u64);
    line_u(c"TRIANGLE".as_ptr(), TRIANGLE as u64);
    line_u(c"TOTAL".as_ptr(), TOTAL);
    line_i(c"NEG".as_ptr(), NEG as i64);
    line_b(c"FLAG".as_ptr(), FLAG);
    line_b(c"OFF".as_ptr(), OFF);
    line_u(c"MASK".as_ptr(), MASK as u64);
    line_i(c"SHIFTED".as_ptr(), SHIFTED);
    line_x(c"HUGE".as_ptr(), HUGE);
    line_x(c"SMALL".as_ptr(), SMALL as u128);
    line_i(c"BLOCK".as_ptr(), BLOCK as i64);
    let () = UNIT;

    line_i(c"PAIR.0".as_ptr(), PAIR.0 as i64);
    line_b(c"PAIR.1".as_ptr(), PAIR.1);
    let (p0, p1) = PAIR;
    line_i(c"p0 + p1".as_ptr(), (p0 + p1 as i32) as i64);
    let ((m, s), (t, ())) = NESTED;
    line_i(c"NESTED".as_ptr(), m as i64 + s as i64 + t as i64);
    line_i(c"FROM_PAIR".as_ptr(), FROM_PAIR as i64);
    line_i(c"FROM_NESTED".as_ptr(), FROM_NESTED as i64);

    line_u(c"FIB_30".as_ptr(), FIB_30);
    line_u(c"FIB_REC".as_ptr(), FIB_REC);
    line_u(c"PAST".as_ptr(), PAST as u64);
    line_u(c"GCD".as_ptr(), GCD);
    line_u(c"ROOT".as_ptr(), ROOT);
    line_u(c"STEPS".as_ptr(), STEPS as u64);
    line_u(c"BITS".as_ptr(), BITS as u64);
    line_b(c"CLASS.0".as_ptr(), CLASS.0);
    line_i(c"CLASS.1".as_ptr(), CLASS.1 as i64);
    line_u(c"POW".as_ptr(), POW as u64);
    line_u(c"ROT".as_ptr(), ROT as u64);

    // Constants as method receivers and in core method arguments.
    line_u(c"LIMIT.pow(2)".as_ptr(), LIMIT.pow(2) as u64);
    line_u(c"SQUARE.count_ones()".as_ptr(), SQUARE.count_ones() as u64);
    line_u(c"TOTAL.max(FIB_30)".as_ptr(), TOTAL.max(FIB_30));
    line_i(c"NEG.rem_euclid(5)".as_ptr(), NEG.rem_euclid(5) as i64);
    line_u(
        c"u32::MAX.rotate_left(LIMIT)".as_ptr(),
        u32::MAX.rotate_left(LIMIT) as u64,
    );

    // Constants in loops.
    let mut i = 0;
    let mut acc = 0u64;
    while i < LIMIT {
        acc += fib(i) * Marker::ID % 97;
        i += 1;
    }
    line_u(c"loop acc".as_ptr(), acc);
    let mut j = NEG;
    let mut count = 0;
    loop {
        if j >= Point::SCALE {
            break;
        }
        j += PAIR.0.abs() / 7;
        count += 1;
    }
    line_i(c"loop count".as_ptr(), count);

    // Struct constants: each use is a new value.
    ORIGIN.describe(c"ORIGIN".as_ptr());
    DIAG.describe(c"DIAG".as_ptr());
    FAR.describe(c"FAR".as_ptr());
    MOVED.describe(c"MOVED".as_ptr());
    Point::UNIT_X.describe(c"Point::UNIT_X".as_ptr());
    Point::SCALED_DIAG.describe(c"Point::SCALED_DIAG".as_ptr());
    let a = FAR;
    let b = FAR;
    line_i(c"take(a) + take(b)".as_ptr(), (take(a) + take(b)) as i64);
    line_i(
        c"take(DIAG) twice".as_ptr(),
        (take(DIAG) + take(DIAG)) as i64,
    );
    line_i(c"show_point(&FAR)".as_ptr(), show_point(&FAR) as i64);
    line_i(c"FAR.x + MOVED.y".as_ptr(), (FAR.x + MOVED.y) as i64);
    let mut m = MOVED;
    m.x += ORIGIN.x + 100;
    m.describe(c"m".as_ptr());
    MOVED.describe(c"MOVED again".as_ptr());
    let Point { x, y } = Point::SCALED_DIAG.scaled(-2);
    line_i(c"x * y".as_ptr(), (x * y) as i64);
    line_i(
        c"area_ratio".as_ptr(),
        Point::new(60, 7).area_ratio() as i64,
    );
    line_i(c"Point::LIMIT_AREA".as_ptr(), Point::LIMIT_AREA as i64);

    SEGMENT.from.describe(c"SEGMENT.from".as_ptr());
    SEGMENT.to.describe(c"SEGMENT.to".as_ptr());
    line_u(c"SEGMENT.color.0".as_ptr(), SEGMENT.color.0 as u64);
    let seg = SEGMENT;
    line_i(c"seg.to.len2()".as_ptr(), seg.to.len2() as i64);
    line_u(c"Rgb::sum(&RED)".as_ptr(), Rgb::sum(&RED) as u64);
    line_u(c"Rgb::WHITE.sum()".as_ptr(), Rgb::WHITE.sum() as u64);
    line_u(c"MARK.id()".as_ptr(), MARK.id());
    line_u(c"Marker::ID".as_ptr(), Marker::ID);

    run_time(LIMIT, FIB_30, 27, NEG);
    run_time(20, 1 << 40, 97, i32::MIN + 1);
    run_time(0, 0, 1, 0);
}
