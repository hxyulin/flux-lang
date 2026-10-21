//! Larger aggregates: structs over 16 bytes with 128-bit fields, structs of
//! nine or more words, deep nesting of structs and tuples, calls with more
//! aggregate pieces than argument registers, structs threaded through
//! several calls, returned from recursive functions, and updated in loops.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

struct Big {
    a: u128,
    b: i128,
    c: u8,
}

struct Nine(u64, u64, u64, u64, u64, u64, u64, u64, u64);

struct Small(u16, u8);

struct Vec2 {
    x: i64,
    y: i64,
}

struct Inner {
    id: u32,
    pos: (Vec2, bool),
}

struct Outer {
    tag: u8,
    inner: Inner,
    pair: (Small, (u64, Small)),
}

struct Stats {
    count: u32,
    sum: i64,
    min: i64,
    max: i64,
    big: Big,
}

fn print_u128(label: *const i8, v: u128) {
    unsafe {
        printf(
            c"%s %016llx%016llx\n".as_ptr(),
            label,
            (v >> 64) as u64,
            v as u64,
        );
    }
}

fn mix_big(b: Big, k: u8) -> Big {
    Big {
        a: b.a * 3 + k as u128,
        b: b.b - (k as i128) * 1000000000000000000000,
        c: b.c ^ k,
    }
}

fn nine_from(seed: u64) -> Nine {
    let s = seed;
    Nine(s, s * 2, s * 3, s * 4, s * 5, s * 6, s * 7, s * 8, s * 9)
}

fn nine_rotate(n: Nine) -> Nine {
    let Nine(w0, w1, w2, w3, w4, w5, w6, w7, w8) = n;
    Nine(w8, w0, w1, w2, w3, w4, w5, w6, w7)
}

fn nine_hash(n: Nine) -> u64 {
    let mut h = 0u64;
    h = h * 31 + n.0;
    h = h * 31 + n.1;
    h = h * 31 + n.2;
    h = h * 31 + n.3;
    h = h * 31 + n.4;
    h = h * 31 + n.5;
    h = h * 31 + n.6;
    h = h * 31 + n.7;
    h * 31 + n.8
}

/// Thirteen aggregate parameters: far more pieces than argument registers.
fn many(
    a: Small,
    b: (u32, u32),
    c: Vec2,
    d: Small,
    e: (u8,),
    f: Vec2,
    g: Big,
    h: Small,
    i: (i64, i64, i64),
    j: Nine,
    k: Small,
    l: (bool, u16),
    m: Vec2,
) -> i64 {
    let low = a.0 as i64 + a.1 as i64 + d.0 as i64 * 10 + d.1 as i64 * 10;
    let small = low + (h.0 as i64 + h.1 as i64) * 100 + (k.0 as i64 + k.1 as i64) * 1000;
    let tuples = b.0 as i64 - b.1 as i64 + e.0 as i64 + i.0 * i.1 - i.2 + l.0 as i64 + l.1 as i64;
    let vecs = c.x + c.y * 2 + f.x * 3 + f.y * 4 + m.x * 5 + m.y * 6;
    let rest = g.c as i64 + (g.a as i64) - (g.b as i64) + nine_hash(j) as i64;
    small + tuples + vecs + rest
}

fn build(depth: u32) -> Outer {
    if depth == 0 {
        Outer {
            tag: 1,
            inner: Inner {
                id: 0,
                pos: (Vec2 { x: 1, y: -1 }, false),
            },
            pair: (Small(1, 2), (3, Small(4, 5))),
        }
    } else {
        let prev = build(depth - 1);
        let Outer {
            tag,
            inner,
            pair: (s, (w, t)),
        } = prev;
        Outer {
            tag: tag * 3,
            inner: Inner {
                id: inner.id + depth,
                pos: (
                    Vec2 {
                        x: inner.pos.0.x * 2,
                        y: inner.pos.0.y - inner.pos.0.x,
                    },
                    !inner.pos.1,
                ),
            },
            pair: (
                Small(t.0 + 1, s.1 + 1),
                (w * 2 + s.0 as u64, Small(s.0 + t.0, t.1)),
            ),
        }
    }
}

fn show_outer(o: Outer) {
    unsafe {
        printf(
            c"outer tag=%u id=%u pos=(%lld,%lld,%d) pair=(%u,%u),(%llu,%u,%u)\n".as_ptr(),
            o.tag as u32,
            o.inner.id,
            o.inner.pos.0.x,
            o.inner.pos.0.y,
            o.inner.pos.1 as i32,
            o.pair.0.0 as u32,
            o.pair.0.1 as u32,
            o.pair.1.0,
            o.pair.1.1.0 as u32,
            o.pair.1.1.1 as u32,
        );
    }
}

fn record(s: Stats, v: i64) -> Stats {
    let min = if s.count == 0 || v < s.min { v } else { s.min };
    let max = if s.count == 0 || v > s.max { v } else { s.max };
    Stats {
        count: s.count + 1,
        sum: s.sum + v,
        min,
        max,
        big: mix_big(s.big, v as u8),
    }
}

fn pass3(s: Stats) -> Stats {
    pass2(s)
}

fn pass2(s: Stats) -> Stats {
    pass1(Stats {
        count: s.count * 10,
        ..s
    })
}

fn pass1(s: Stats) -> Stats {
    s
}

fn main() {
    let b = mix_big(
        mix_big(
            Big {
                a: 340282366920938463463374607431768211455 / 5,
                b: -170141183460469231731687303715884105728 + 7,
                c: 0xa5,
            },
            9,
        ),
        200,
    );
    print_u128(c"big.a".as_ptr(), b.a);
    print_u128(c"big.b".as_ptr(), b.b as u128);
    unsafe {
        printf(c"big.c %u\n".as_ptr(), b.c as u32);
    }

    let mut n = nine_from(11);
    let mut r = 0;
    while r < 4 {
        n = nine_rotate(n);
        r += 1;
    }
    n.4 += 1000;
    unsafe {
        printf(c"nine %llu %llu %llu\n".as_ptr(), n.0, n.4, n.8);
    }
    let h = nine_hash(n);
    unsafe {
        printf(c"nine hash %llu\n".as_ptr(), h);
    }

    let total = many(
        Small(1, 2),
        (100, 40),
        Vec2 { x: 3, y: 4 },
        Small(5, 6),
        (7,),
        Vec2 { x: -8, y: 9 },
        Big {
            a: 1000,
            b: -2000,
            c: 3,
        },
        Small(10, 11),
        (12, 13, 14),
        nine_from(1),
        Small(15, 16),
        (true, 17),
        Vec2 { x: 18, y: -19 },
    );
    unsafe {
        printf(c"many %lld\n".as_ptr(), total);
    }

    let mut d = 0;
    while d < 5 {
        show_outer(build(d));
        d += 2;
    }

    let mut s = Stats {
        count: 0,
        sum: 0,
        min: 0,
        max: 0,
        big: Big { a: 1, b: -1, c: 0 },
    };
    let mut v: i64 = 17;
    let mut i = 0;
    while i < 12 {
        s = record(s, v);
        v = (v * 37 + 11) % 101 - 50;
        i += 1;
    }
    s = pass3(s);
    unsafe {
        printf(
            c"stats %u %lld %lld %lld %u\n".as_ptr(),
            s.count,
            s.sum,
            s.min,
            s.max,
            s.big.c as u32,
        );
    }
    print_u128(c"stats.a".as_ptr(), s.big.a);
    print_u128(c"stats.b".as_ptr(), s.big.b as u128);
}
