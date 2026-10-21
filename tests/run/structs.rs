//! Named-field, tuple and unit structs: literals with shorthand and
//! functional update, field reads and writes, compound assignment on
//! fields, struct patterns in `let` and parameters, and structs passed to
//! and returned from functions in ABI-sensitive shapes.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

struct Unit;

struct Empty {}

struct Flag {
    on: bool,
}

struct One {
    v: u32,
}

struct Pair {
    a: u8,
    b: u64,
}

struct Mixed3 {
    x: u16,
    y: i32,
    z: u8,
}

struct Mixed4 {
    a: u8,
    b: u16,
    c: u32,
    d: u64,
}

struct Wide {
    lo: u64,
    hi: i128,
}

struct Eight {
    a: u8,
    b: i8,
    c: u16,
    d: i16,
    e: u32,
    f: i32,
    g: u64,
    h: i64,
}

struct Point(i32, i32);

struct Label(*const i8, u32);

struct Rect {
    origin: Point,
    size: Point,
    name: Label,
}

fn unit_count(_u: Unit, _e: Empty, n: u32) -> u32 {
    n + 1
}

fn toggle(f: Flag) -> Flag {
    Flag { on: !f.on }
}

fn bump(o: One) -> One {
    One { v: o.v * 3 + 1 }
}

fn pair_sum(p: Pair) -> u64 {
    p.a as u64 + p.b
}

fn make_pair(a: u8, b: u64) -> Pair {
    Pair { a, b }
}

fn mixed3(m: Mixed3) -> Mixed3 {
    Mixed3 {
        x: m.x + 1,
        y: m.y - 2,
        z: m.z * 2,
    }
}

fn mixed4_total(Mixed4 { a, b, c, d }: Mixed4) -> u64 {
    a as u64 + b as u64 + c as u64 + d
}

fn wide(w: Wide) -> Wide {
    Wide {
        lo: w.lo + 1,
        hi: w.hi * -3,
    }
}

fn eight(e: Eight) -> i64 {
    e.a as i64 + e.b as i64 + e.c as i64 + e.d as i64 + e.e as i64 + e.f as i64 + e.g as i64 + e.h
}

fn area(r: Rect) -> i32 {
    let Rect {
        size: Point(w, h), ..
    } = r;
    w * h
}

fn shift(Point(x, y): Point, dx: i32) -> Point {
    Point(x + dx, y - dx)
}

fn show_point(label: *const i8, p: Point) {
    unsafe {
        printf(c"%s=(%d,%d)\n".as_ptr(), label, p.0, p.1);
    }
}

fn main() {
    let n = unit_count(Unit, Empty {}, 41);
    unsafe {
        printf(c"unit %u\n".as_ptr(), n);
    }

    let f = toggle(Flag { on: false });
    let g = toggle(toggle(Flag { on: true }));
    unsafe {
        printf(c"flags %d %d\n".as_ptr(), f.on as i32, g.on as i32);
    }

    let o = bump(bump(One { v: 5 }));
    unsafe {
        printf(c"one %u\n".as_ptr(), o.v);
    }

    let p = make_pair(200, 1 << 40);
    let q = Pair {
        a: 255,
        ..make_pair(1, 7)
    };
    unsafe {
        printf(c"pair %llu %llu\n".as_ptr(), pair_sum(p), pair_sum(q));
    }

    let mut m = mixed3(Mixed3 {
        x: 65535,
        y: -2147483647,
        z: 130,
    });
    unsafe {
        printf(c"mixed3 %u %d %u\n".as_ptr(), m.x as u32, m.y, m.z as u32);
    }
    m.x += 7;
    m.y *= -1;
    m.z = m.z / 3;
    unsafe {
        printf(c"mixed3 %u %d %u\n".as_ptr(), m.x as u32, m.y, m.z as u32);
    }

    let base = Mixed4 {
        a: 1,
        b: 2,
        c: 3,
        d: 4,
    };
    let m4 = Mixed4 { c: 300000, ..base };
    unsafe {
        printf(c"mixed4 %llu\n".as_ptr(), mixed4_total(m4));
    }

    let w = wide(Wide {
        lo: 18446744073709551615,
        hi: -12345678901234567890123,
    });
    unsafe {
        printf(
            c"wide %llu %llx %llx\n".as_ptr(),
            w.lo,
            (w.hi >> 64) as u64,
            w.hi as u64,
        );
    }

    let mut e = Eight {
        a: 250,
        b: -100,
        c: 60000,
        d: -30000,
        e: 4000000000,
        f: -2000000000,
        g: 1,
        h: -1,
    };
    e.g += 1 << 50;
    e.h -= 5;
    let e2 = Eight { a: 1, b: 1, ..e };
    unsafe {
        printf(c"eight %lld\n".as_ptr(), eight(e2));
    }

    let mut r = Rect {
        origin: Point(1, 2),
        size: Point(30, 40),
        name: Label(c"box".as_ptr(), 9),
    };
    r.size.0 += 5;
    r.origin.1 = -r.origin.1;
    unsafe {
        printf(
            c"rect %s#%u at (%d,%d)\n".as_ptr(),
            r.name.0,
            r.name.1,
            r.origin.0,
            r.origin.1,
        );
    }
    let a = area(r);
    unsafe {
        printf(c"area %d\n".as_ptr(), a);
    }

    let s: Point;
    s = shift(Point(10, 20), 3);
    show_point(c"shifted".as_ptr(), s);

    let Point(mut px, py) = shift(Point(-1, -1), -4);
    px *= 2;
    show_point(c"rebuilt".as_ptr(), Point(px, py));

    let Pair { a: renamed, b: _ } = make_pair(77, 0);
    let One { v } = One {
        v: renamed as u32 + 1,
    };
    unsafe {
        printf(c"patterns %u %u\n".as_ptr(), renamed as u32, v);
    }
}
