//! References: shared and mutable borrows of scalars, structs with mixed
//! field widths, nested structs and tuples, fields and temporaries; reads
//! and writes through `*r`, auto-deref of fields, reborrow chains through
//! several calls, borrows taken repeatedly in loops, borrowed parameters,
//! reads after the referent changed once a borrow ended, and recursion
//! with `&mut` accumulators.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

struct Mixed {
    a: u8,
    b: u16,
    c: u32,
    d: u64,
    e: i128,
    f: bool,
}

struct Inner {
    x: i32,
    y: (u8, i64),
}

struct Outer {
    tag: u16,
    inner: Inner,
    pair: (Inner, u32),
}

struct Counter(u64, u8);

fn print_i128(label: *const i8, v: i128) {
    unsafe {
        printf(
            c"%s %016llx%016llx\n".as_ptr(),
            label,
            (v >> 64) as u64,
            v as u64,
        );
    }
}

fn read(r: &i32) -> i32 {
    *r * 2
}

fn add_to(r: &mut i32, n: i32) {
    *r += n;
}

fn add_twice(r: &mut i32, n: i32) {
    add_to(r, n);
    add_to(&mut *r, n);
}

fn add_four(r: &mut i32, n: i32) {
    add_twice(r, n);
    add_twice(r, n + 1);
}

fn sum_mixed(m: &Mixed) -> u64 {
    m.a as u64 + m.b as u64 + m.c as u64 + m.d + m.e as u64 + m.f as u64
}

fn touch_mixed(m: &mut Mixed) {
    m.a = m.a + 200;
    m.b *= 3;
    m.c ^= 0xdead_beef;
    (*m).d += 1 << 40;
    m.e = -m.e * 7;
    m.f = !m.f;
}

fn show_mixed(label: *const i8, m: &Mixed) {
    unsafe {
        printf(
            c"%s a=%u b=%u c=%u d=%llu f=%d\n".as_ptr(),
            label,
            m.a as u32,
            m.b as u32,
            m.c,
            m.d,
            m.f as i32,
        );
    }
    print_i128(label, m.e);
}

fn bump_u8(r: &mut u8) {
    *r = *r * 3 + 1;
}

fn bump_i64(r: &mut i64) {
    *r -= 1000;
}

fn deep(o: &mut Outer, k: i32) {
    o.tag += 1;
    o.inner.x *= k;
    bump_u8(&mut o.inner.y.0);
    bump_i64(&mut o.pair.0.y.1);
    o.pair.1 += o.inner.x as u32;
}

fn sum_outer(o: &Outer) -> i64 {
    o.tag as i64
        + o.inner.x as i64
        + o.inner.y.0 as i64
        + o.inner.y.1
        + o.pair.0.x as i64
        + o.pair.0.y.0 as i64
        + o.pair.0.y.1
        + o.pair.1 as i64
}

fn inner_sum(i: &Inner) -> i64 {
    i.x as i64 + i.y.0 as i64 + i.y.1
}

fn make_inner(x: i32) -> Inner {
    Inner {
        x,
        y: (x as u8, x as i64 * 1000),
    }
}

fn tick(c: &mut Counter) {
    c.0 += c.1 as u64;
    c.1 = c.1 * 5 + 3;
}

fn bumped(mut m: Mixed) -> Mixed {
    touch_mixed(&mut m);
    m
}

fn swap(a: &mut i64, b: &mut i64) {
    let t = *a;
    *a = *b;
    *b = t;
}

fn collatz(n: u64, steps: &mut u32, peak: &mut u64) {
    if n > *peak {
        *peak = n;
    }
    if n != 1 {
        *steps += 1;
        if n % 2 == 0 {
            collatz(n / 2, steps, peak);
        } else {
            collatz(3 * n + 1, &mut *steps, &mut *peak);
        }
    }
}

fn fib_into(n: u32, acc: &mut (u64, u64)) {
    if n > 0 {
        let next = acc.0 + acc.1;
        acc.0 = acc.1;
        acc.1 = next;
        fib_into(n - 1, acc);
    }
}

fn wide_mul(r: &mut i128, by: &i128) {
    *r *= *by;
}

fn main() {
    // Scalars.
    let mut x = 10;
    add_to(&mut x, 5);
    let shared = &x;
    let copy = shared;
    let doubled = read(shared) + read(copy) + *shared;
    unsafe {
        printf(c"scalar %d %d\n".as_ptr(), x, doubled);
    }
    add_four(&mut x, 2);
    {
        let r = &mut x;
        *r *= 3;
        *r += 1;
        let rr = &mut *r;
        *rr -= 4;
    }
    let after = read(&x);
    unsafe {
        printf(c"chain %d %d\n".as_ptr(), x, after);
    }

    // Mixed widths through references.
    let mut m = Mixed {
        a: 100,
        b: 40000,
        c: 0x1234_5678,
        d: 1 << 60,
        e: -170141183460469231731687303715884105727,
        f: false,
    };
    show_mixed(c"m0".as_ptr(), &m);
    touch_mixed(&mut m);
    show_mixed(c"m1".as_ptr(), &m);
    let mr = &mut m;
    mr.c += 1;
    touch_mixed(mr);
    let s = sum_mixed(&m);
    unsafe {
        printf(c"mixed sum %llu\n".as_ptr(), s);
    }
    let b = bumped(m);
    show_mixed(c"bumped".as_ptr(), &b);

    // Nested structs and tuples; borrowing fields.
    let mut o = Outer {
        tag: 65534,
        inner: make_inner(7),
        pair: (make_inner(-3), 100),
    };
    deep(&mut o, 4);
    deep(&mut o, -2);
    {
        let fx = &mut o.inner.x;
        *fx += 1000;
    }
    let fy = &mut o.pair.0.y;
    fy.0 = fy.0 - 10;
    fy.1 *= 3;
    let t = &mut o.pair;
    t.1 -= 1;
    unsafe {
        printf(
            c"outer %u %d %u %lld\n".as_ptr(),
            o.tag as u32,
            o.inner.x,
            o.inner.y.0 as u32,
            o.inner.y.1,
        );
        printf(
            c"outer pair %d %u %lld %u sum %lld\n".as_ptr(),
            o.pair.0.x,
            o.pair.0.y.0 as u32,
            o.pair.0.y.1,
            o.pair.1,
            sum_outer(&o),
        );
    }
    let ir = &o.pair.0;
    let inner_total = inner_sum(ir) + inner_sum(&o.inner);
    unsafe {
        printf(c"inner %lld\n".as_ptr(), inner_total);
    }

    // Borrows of temporaries.
    let tmp = &make_inner(42);
    let tmp_sum = inner_sum(tmp) + inner_sum(&make_inner(-1));
    let lit = &5;
    let lit_sum = *lit + read(&(*lit + 1));
    unsafe {
        printf(c"temps %lld %d %d\n".as_ptr(), tmp_sum, tmp.x, lit_sum);
    }

    // Loops that borrow and mutate.
    let mut c = Counter(0, 1);
    let mut i = 0;
    while i < 20 {
        tick(&mut c);
        let r = &c;
        if r.1 % 2 == 0 {
            tick(&mut c);
        }
        i += 1;
    }
    unsafe {
        printf(c"counter %llu %u\n".as_ptr(), c.0, c.1 as u32);
    }

    let mut p = 1;
    let mut q = 2;
    let mut n = 0;
    while n < 7 {
        swap(&mut p, &mut q);
        let pr = &mut p;
        *pr += n;
        n += 1;
    }
    unsafe {
        printf(c"swap %lld %lld\n".as_ptr(), p, q);
    }

    // NLL: the referent changes once the borrow has ended.
    let mut v = 3;
    let vr = &v;
    let seen = *vr;
    v += 10;
    let vr2 = &v;
    unsafe {
        printf(c"nll %d %d\n".as_ptr(), seen, *vr2);
    }

    // Recursion with accumulators.
    let mut steps = 0;
    let mut peak = 0;
    collatz(27, &mut steps, &mut peak);
    let mut acc = (0, 1);
    fib_into(90, &mut acc);
    unsafe {
        printf(c"collatz %u %llu fib %llu\n".as_ptr(), steps, peak, acc.0);
    }

    // 128-bit values through references.
    let mut w: i128 = -3;
    let by = 1_000_000_007;
    let mut k = 0;
    while k < 5 {
        wide_mul(&mut w, &by);
        k += 1;
    }
    print_i128(c"wide".as_ptr(), w);
}
