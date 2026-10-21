//! Borrowed locals that live in stack memory: results returned through
//! sret into the place they were computed from, assignments through `&mut`
//! whose right side reads the same place, ten reference arguments of every
//! width, disjoint borrows of fields, locals reassigned after a move and
//! borrowed again, zero-sized borrows, borrowed locals in recursive frames,
//! and borrows taken in branches, loops and `break` values.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

struct V(i64, i64, i64);

impl V {
    fn rot(&self) -> V {
        V(self.1, self.2, self.0)
    }

    fn rot_val(self) -> V {
        V(self.1, self.2, self.0)
    }

    fn get(&self) -> i64 {
        self.0
    }

    fn set(&mut self, v: i64) {
        self.0 = v;
    }

    fn rot_in_place(&mut self) {
        *self = self.rot();
        *self = rot_ref(self);
    }
}

struct W {
    inner: V,
    k: u8,
}

struct S {
    a: u8,
    b: u32,
    c: (u16, (i64, bool)),
}

struct Unit;

impl Unit {
    fn id(&self, n: u32) -> u32 {
        n + 1
    }

    fn double(&mut self, n: &mut u32) {
        *n *= 2;
    }
}

struct Z {
    u: Unit,
    x: u32,
    e: (),
}

fn rot_ref(v: &V) -> V {
    V(v.2 * 10, v.0, v.1)
}

fn twist(r: &mut V) {
    *r = rot_ref(r);
    *r = r.rot();
    r.rot_in_place();
}

fn many(
    a: &u8,
    b: &mut u16,
    c: &u32,
    d: &mut u64,
    e: &i128,
    f: &mut bool,
    g: &i8,
    h: &mut i16,
    i: &i32,
    j: &mut i64,
) -> i64 {
    *b += *a as u16;
    *d += *c as u64;
    *f = !*f;
    *h -= *g as i16;
    *j += *i as i64 + *e as i64;
    *a as i64
        + *b as i64
        + *c as i64
        + *d as i64
        + *e as i64
        + *f as i64
        + *g as i64
        + *h as i64
        + *i as i64
        + *j
}

fn split(x: &mut u32, y: &(u16, (i64, bool))) {
    *x += y.0 as u32 + y.1.0 as u32;
    if y.1.1 {
        *x *= 2;
    }
}

fn take_unit(_u: &(), n: u32) -> u32 {
    n + 3
}

/// Each frame borrows its own local; the callee must not clobber it.
fn tree(depth: u32, total: &mut u64) -> u64 {
    let mut here = depth as u64 * 10 + 1;
    let h = &mut here;
    *total += *h;
    if depth > 0 {
        *h += tree(depth - 1, total);
        *h += tree(depth - 1, &mut *total);
    }
    here
}

fn is_even(n: u32, calls: &mut u32) -> bool {
    *calls += 1;
    if n == 0 { true } else { is_odd(n - 1, calls) }
}

fn is_odd(n: u32, calls: &mut u32) -> bool {
    *calls += 1;
    if n == 0 { false } else { is_even(n - 1, calls) }
}

fn first_over(limit: i64, step: &i64) -> i64 {
    let mut x = 1;
    loop {
        let r = &mut x;
        *r = *r * 2 + *step;
        if x > limit {
            break x * 10;
        }
    }
}

fn find(limit: u32) -> u32 {
    let mut i = 0;
    while i < 100 {
        let r = &i;
        if *r * *r > limit {
            return *r;
        }
        i += 1;
    }
    0
}

fn main() {
    // Results written back to the borrowed place they were computed from.
    let mut v = V(1, 2, 3);
    let r = &v;
    let first = r.get();
    v = v.rot();
    v = rot_ref(&v);
    v = v.rot();
    v = v.rot_val();
    twist(&mut v);
    let vr = &mut v;
    *vr = vr.rot();
    twist(vr);
    let mut w = W {
        inner: V(10, 20, 30),
        k: 5,
    };
    w.inner = w.inner.rot();
    w.inner.set(w.inner.get() + w.k as i64);
    let wr = &mut w;
    wr.inner = wr.inner.rot();
    wr.inner = rot_ref(&wr.inner);
    wr.k += 1;
    unsafe {
        printf(c"alias %lld %lld %lld %lld\n".as_ptr(), first, v.0, v.1, v.2);
        printf(
            c"alias w %lld %lld %lld %u\n".as_ptr(),
            w.inner.0,
            w.inner.1,
            w.inner.2,
            w.k as u32,
        );
    }

    // Ten reference arguments, more than fit in registers.
    let a = 200u8;
    let mut b = 60000u16;
    let c = 4000000000u32;
    let mut d = 1u64 << 63;
    let e = -5i128;
    let mut f = true;
    let g = -100i8;
    let mut h = -32000i16;
    let i = 123456;
    let mut j = -1i64;
    let r1 = many(&a, &mut b, &c, &mut d, &e, &mut f, &g, &mut h, &i, &mut j);
    let r2 = many(&a, &mut b, &c, &mut d, &e, &mut f, &g, &mut h, &i, &mut j);
    unsafe {
        printf(
            c"many %lld %lld %u %llu %d %d %lld\n".as_ptr(),
            r1,
            r2,
            b as u32,
            d,
            f as i32,
            h as i32,
            j,
        );
    }

    // Disjoint field borrows, then a move out of a field.
    let mut s = S {
        a: 1,
        b: 2,
        c: (3, (4, true)),
    };
    split(&mut s.b, &s.c);
    let sr = &mut s;
    sr.a = 9;
    sr.c.1.0 -= 50;
    let sum = s.a as i64 + s.c.1.0;
    let t = &mut s.c.1.1;
    *t = false;
    split(&mut s.b, &s.c);
    let moved = s.c;
    let bb = &s.b;
    unsafe {
        printf(
            c"split %lld %u %u %lld %d\n".as_ptr(),
            sum,
            *bb,
            moved.0 as u32,
            moved.1.0,
            moved.1.1 as i32,
        );
    }

    let mut tt = (1u8, ((2u16, 3i64), 4u32));
    let deep = &mut tt.1.0.1;
    *deep *= -7;
    let deep2 = &mut tt.1;
    deep2.1 += 10;
    deep2.0.0 += 1;
    let tmp = &mut (1, 2);
    tmp.0 += 1;
    tmp.1 *= 5;
    let (p, q) = *tmp;
    unsafe {
        printf(
            c"tuples %u %u %lld %u %d %d %d\n".as_ptr(),
            tt.0 as u32,
            tt.1.0.0 as u32,
            tt.1.0.1,
            tt.1.1,
            tmp.0,
            tmp.1,
            p * q,
        );
    }

    // Reassigned after a move, borrowed again.
    let x;
    x = 5;
    let xr = &x;
    let mut s = S {
        a: 3,
        b: 4,
        c: (5, (6, false)),
    };
    s.b += 1;
    let s2 = s;
    s = S {
        a: 10,
        b: 20,
        c: (30, (40, true)),
    };
    let sr = &mut s;
    sr.a += 1;
    let mut total = s2.a as i64 + s2.b as i64 + s.a as i64 + s.c.1.0 + *xr;
    s = S { a: 100, ..s2 };
    let sr = &s;
    total += sr.a as i64 + sr.c.0 as i64;
    unsafe {
        printf(c"reassign %lld %d\n".as_ptr(), total, s.c.1.1 as i32);
    }

    // Zero-sized values and fields.
    let mut z = Z {
        u: Unit,
        x: 5,
        e: (),
    };
    let ur = &mut z.u;
    let mut n = ur.id(1);
    ur.double(&mut n);
    z.u.double(&mut z.x);
    let er = &z.e;
    let k = take_unit(er, z.x) + take_unit(&(), 0);
    let mut u = Unit;
    let u2 = &mut u;
    u2.double(&mut n);
    let m = u.id(n);
    unsafe {
        printf(c"zst %u %u %u %u\n".as_ptr(), n, z.x, k, m);
    }

    // Recursion.
    let mut acc = 0;
    let root = tree(4, &mut acc);
    let mut calls = 0;
    let even = is_even(17, &mut calls);
    let odd = is_odd(9, &mut calls);
    unsafe {
        printf(
            c"recursion %llu %llu %d %d %u\n".as_ptr(),
            root,
            acc,
            even as i32,
            odd as i32,
            calls,
        );
    }

    // Branches, `break` values and early returns.
    let mut cond = 0;
    let mut bw = 1;
    while cond < 6 {
        if cond % 2 == 0 {
            let br = &mut bw;
            *br *= 3;
        } else {
            bw += 1;
        }
        cond += 1;
    }
    let step = 3;
    unsafe {
        printf(
            c"control %d %lld %u %u\n".as_ptr(),
            bw,
            first_over(1000, &step),
            find(50),
            find(2000000),
        );
    }
}
