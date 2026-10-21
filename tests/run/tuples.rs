//! Tuples: unit and 1-tuples, mixed widths, nesting, field access and
//! assignment, compound assignment through nested fields, tuple patterns
//! with `_`, `..` and `mut`, tuples as parameters and return values, and
//! copies of tuples of Copy types.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn nothing(u: ()) -> () {
    u
}

fn single(t: (u32,)) -> (u32,) {
    (t.0 * 2,)
}

fn swap(t: (i32, i64)) -> (i64, i32) {
    (t.1, t.0)
}

fn divmod(a: u32, b: u32) -> (u32, u32) {
    (a / b, a % b)
}

fn widths(t: (u8, u16, u32, u64)) -> u64 {
    let (a, b, c, d) = t;
    a as u64 * 1000000 + b as u64 * 1000 + c as u64 + d
}

fn big(t: (i128, u8, u128)) -> (u8, i128) {
    let (x, y, z) = t;
    (y + 1, x + z as i128)
}

fn bools(t: (bool, bool, bool)) -> u32 {
    let (a, _, c) = t;
    let (.., last) = t;
    (a as u32) + 2 * (c as u32) + 4 * (last as u32)
}

fn nested((a, (b, c)): (i32, (i32, i32)), (d, ..): (i32, i32, i32)) -> i32 {
    a * 1000 + b * 100 + c * 10 + d
}

fn fib_pair(n: u32) -> (u64, u64) {
    let mut s = (0u64, 1u64);
    let mut i = 0;
    while i < n {
        s = (s.1, s.0 + s.1);
        i += 1;
    }
    s
}

fn minmax(a: i32, b: i32, c: i32) -> (i32, i32) {
    let mut r = (a, a);
    if b < r.0 {
        r.0 = b;
    }
    if c < r.0 {
        r.0 = c;
    }
    if b > r.1 {
        r.1 = b;
    }
    if c > r.1 {
        r.1 = c;
    }
    r
}

fn main() {
    let u = nothing(());
    let ((), one) = (u, single((21,)));
    unsafe {
        printf(c"single %u\n".as_ptr(), one.0);
    }

    let (x, y) = swap((-5, 1 << 33));
    unsafe {
        printf(c"swap %lld %d\n".as_ptr(), x, y);
    }

    let (q, r) = divmod(1000, 7);
    let qr = divmod(q, 3);
    unsafe {
        printf(c"divmod %u %u %u %u\n".as_ptr(), q, r, qr.0, qr.1);
    }

    unsafe {
        printf(c"widths %llu\n".as_ptr(), widths((255, 65535, 7, 1 << 40)));
    }

    let b = big((-1, 254, 1 << 100));
    unsafe {
        printf(
            c"big %u %llx %llx\n".as_ptr(),
            b.0 as u32,
            (b.1 >> 64) as u64,
            b.1 as u64,
        );
    }

    unsafe {
        printf(
            c"bools %u %u %u\n".as_ptr(),
            bools((true, false, true)),
            bools((false, true, false)),
            bools((true, true, false)),
        );
    }

    unsafe {
        printf(c"nested %d\n".as_ptr(), nested((1, (2, 3)), (4, 5, 6)));
    }

    let f = fib_pair(90);
    unsafe {
        printf(c"fib %llu %llu\n".as_ptr(), f.0, f.1);
    }

    let mm = minmax(3, -9, 12);
    unsafe {
        printf(c"minmax %d %d\n".as_ptr(), mm.0, mm.1);
    }

    let mut t = ((1u8, 2u16), (3u32, (4u64, -5i64)), true);
    t.0.0 += 250;
    t.0.1 *= 1000;
    t.1.1.0 <<= 60;
    t.1.1.1 -= 10;
    t.2 = !t.2;
    t.1.0 = t.1.0 + t.0.0 as u32;
    unsafe {
        printf(
            c"deep %u %u %u %llu %lld %d\n".as_ptr(),
            t.0.0 as u32,
            t.0.1 as u32,
            t.1.0,
            t.1.1.0,
            t.1.1.1,
            t.2 as i32,
        );
    }

    let copy = t;
    t.1.0 = 0;
    let (_, (mut c0, _), _) = copy;
    c0 += 1;
    unsafe {
        printf(c"copy %u %u %u\n".as_ptr(), copy.1.0, t.1.0, c0);
    }

    let late: (i16, *const i8);
    late = (-300, c"late".as_ptr());
    let (n, s) = late;
    unsafe {
        printf(c"%s %d %d\n".as_ptr(), s, n as i32, late.0 as i32);
    }

    let mut acc = (0u32, 0u64);
    let mut k = 1;
    while k <= 10 {
        acc.0 += k;
        acc.1 += (k * k) as u64;
        k += 1;
    }
    let (sum, squares) = acc;
    unsafe {
        printf(c"acc %u %llu\n".as_ptr(), sum, squares);
    }
}
