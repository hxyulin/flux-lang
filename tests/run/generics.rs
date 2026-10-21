//! Generic functions: identity and selection over many types (u8, i16,
//! u64, i128, bool, unit, tuples, structs with mixed field sizes),
//! several type parameters, turbofish and `_` in body types, assignment
//! through `&mut T`, generic functions calling generic functions with the
//! same and with derived type arguments, generic recursion and mutual
//! recursion on the same instance with runtime base cases, references
//! passed as type arguments to functions that return no reference, and
//! generic functions in nested modules calling through `super`.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
    fn exit(code: i32) -> !;
}

struct Mixed {
    a: u8,
    b: u64,
    c: i16,
    d: bool,
}

fn id<T>(x: T) -> T {
    x
}

fn pick<T>(first: bool, a: T, b: T) -> T {
    if first { a } else { b }
}

fn swap<A, B>(p: (A, B)) -> (B, A) {
    let (a, b) = p;
    (b, a)
}

fn pair<A, B>(a: A, b: B) -> (A, B) {
    (a, b)
}

fn triple<T>(a: T, b: T, c: T) -> (T, T, T) {
    (a, b, c)
}

// Generic functions calling generic functions: same T, and tuple of T.
fn id_twice<T>(x: T) -> T {
    id(id::<T>(x))
}

fn rotate<T>(t: (T, T, T)) -> (T, T, T) {
    let (a, b, c) = t;
    triple(b, c, a)
}

fn rotate_n<T>(n: u32, t: (T, T, T)) -> (T, T, T) {
    if n == 0 {
        t
    } else {
        rotate_n(n - 1, rotate(t))
    }
}

// Recursion on the same instance: alternates `a` and `b`.
fn nth<T>(n: u32, a: T, b: T) -> T {
    if n == 0 { a } else { nth(n - 1, b, a) }
}

// Mutual recursion between two generic functions on the same T.
fn even_pick<T>(n: u32, yes: T, no: T) -> T {
    if n == 0 {
        yes
    } else {
        odd_pick(n - 1, yes, no)
    }
}

fn odd_pick<T>(n: u32, yes: T, no: T) -> T {
    if n == 0 {
        no
    } else {
        even_pick(n - 1, yes, no)
    }
}

fn set<T>(slot: &mut T, v: T) {
    *slot = v;
}

fn set_both<A, B>(p: &mut (A, B), a: A, b: B) {
    set(&mut p.0, a);
    p.1 = b;
}

fn first_of<A, B, C>(a: A, b: B, c: C) -> A {
    let _unused = (b, c);
    a
}

fn drop_it<T>(_x: T) {}

fn take_ref<T>(x: T) -> u32 {
    drop_it(x);
    7
}

fn count<T>(n: u32, x: T) -> u32 {
    if n == 0 {
        drop_it(x);
        0
    } else {
        1 + count(n - 1, x)
    }
}

fn wide(v: (u8, i128, u16)) -> (u16, i128, u8) {
    let (a, b, c) = id(v);
    (c, b, a)
}

mod util {
    pub fn choose<T>(n: u8, a: T, b: T, c: T) -> T {
        if n == 0 {
            a
        } else if n == 1 {
            b
        } else {
            c
        }
    }

    pub mod deep {
        pub fn wrap<T>(x: T) -> (T,) {
            (hidden(x),)
        }

        fn hidden<T>(x: T) -> T {
            super::super::id(x)
        }
    }
}

fn mixed(seed: u8) -> Mixed {
    Mixed {
        a: seed,
        b: seed as u64 * 1_000_000_007,
        c: -(seed as i16) * 3,
        d: seed % 2 == 1,
    }
}

fn show_mixed(m: &Mixed) {
    unsafe {
        printf(
            c"mixed %u %llu %d %d\n".as_ptr(),
            m.a as u32,
            m.b,
            m.c as i32,
            m.d as i32,
        );
    }
}

fn main() {
    let a = id(200u8);
    let b = id::<i16>(-30000);
    let c: u64 = id(0xdead_beef_cafe_f00d);
    let d = id(-170141183460469231731687303715884105728i128 + 12345);
    let e = id(true);
    let f = id_twice(false);
    let () = id(());
    let u: () = id_twice::<()>(());
    let _ = u;
    unsafe {
        printf(
            c"id %u %d %llx %llx %llx %d %d\n".as_ptr(),
            a as u32,
            b as i32,
            c,
            (d >> 64) as u64,
            d as u64,
            e as i32,
            f as i32,
        );
    }

    // Selection between moved values.
    let p8 = pick(true, 1u8, 2u8);
    let p16 = pick::<i16>(false, -1, -2);
    let p128: i128 = pick(false, 1 << 100, -(1 << 90));
    let pb = pick(false, true, false);
    let pt = pick(true, (1u8, 2u64, -3i32), (4, 5, 6));
    let pm = pick(false, mixed(1), mixed(250));
    unsafe {
        printf(
            c"pick %u %d %llx %llx %d %u %llu %d\n".as_ptr(),
            p8 as u32,
            p16 as i32,
            (p128 >> 64) as u64,
            p128 as u64,
            pb as i32,
            pt.0 as u32,
            pt.1,
            pt.2,
        );
    }
    show_mixed(&pm);

    // Several type parameters, tuples in and out.
    let s1 = swap((1u8, 2i128));
    let s2 = swap(swap((true, -9i16)));
    let s3: (u64, (bool, u8)) = swap(pair((false, 77u8), 99u64));
    let s4 = swap::<(), u16>(((), 65535));
    unsafe {
        printf(
            c"swap %lld %u %d %d %llu %d %u %u\n".as_ptr(),
            s1.0 as i64,
            s1.1 as u32,
            s2.0 as i32,
            s2.1 as i32,
            s3.0,
            (s3.1).0 as i32,
            (s3.1).1 as u32,
            s4.0 as u32,
        );
    }
    let inferred: (_, _) = pair(id::<_>(3u8), -7i64);
    let mut slot = 1u16;
    set(&mut slot, 999);
    let mut sp = (0u8, mixed(0));
    set_both(&mut sp, 5, mixed(6));
    set(&mut sp.1.c, -1234);
    set::<bool>(&mut sp.1.d, false);
    let mut big: (i128, ()) = (0, ());
    set_both(&mut big, -(1 << 99), ());
    unsafe {
        printf(
            c"set %u %lld %u %u %llx\n".as_ptr(),
            inferred.0 as u32,
            inferred.1,
            slot as u32,
            sp.0 as u32,
            (big.0 >> 64) as u64,
        );
    }
    show_mixed(&sp.1);
    let ws = wide((3, -5, 1000));
    unsafe {
        printf(
            c"wide %u %lld %u\n".as_ptr(),
            ws.0 as u32,
            ws.1 as i64,
            ws.2 as u32,
        );
    }

    // Recursion with runtime base cases.
    let mut n = 0;
    while n < 5 {
        let x = nth(n, 10u8, 20u8);
        let y = nth(n, (1i128 << 70, 1u8), (-1, 2));
        let z = even_pick(n, 101u32, 111u32);
        let w = odd_pick(n, mixed(n as u8), mixed(100 + n as u8));
        let (r0, r1, r2) = rotate_n(n, triple(1u16, 2, 3));
        let (q0, _q1, q2) = rotate_n(n * 2, (pair(1u8, true), pair(2, false), pair(3, true)));
        unsafe {
            printf(
                c"rec %u: %u %lld %u %c %u%u%u %u%d %u%d\n".as_ptr(),
                n,
                x as u32,
                (y.0 >> 64) as i64,
                y.1 as u32,
                z,
                r0 as u32,
                r1 as u32,
                r2 as u32,
                q0.0 as u32,
                q0.1 as i32,
                q2.0 as u32,
                q2.1 as i32,
            );
        }
        show_mixed(&w);
        n += 1;
    }

    // References as type arguments to functions returning no reference.
    let local = 5i64;
    let mut m = mixed(9);
    let k = take_ref(&local) + take_ref(&mut m) + take_ref(&local);
    let cnt = count(6, &m) + count(3, mixed(1)) + count::<()>(2, ());
    m.a += 1;
    show_mixed(&m);
    let fo = first_of(-4i16, &m, mixed(3));
    unsafe {
        printf(c"refs %u %u %d\n".as_ptr(), k, cnt, fo as i32);
    }

    // Generic functions in modules.
    let ch = util::choose(1, 10u16, 20, 30) as u32
        + util::choose::<u32>(2, 1, 2, 3)
        + util::choose(0, (5u8, 6u64), (7, 8), (9, 10)).1 as u32;
    let wr = util::deep::wrap(mixed(4));
    show_mixed(&wr.0);
    let wb = util::deep::wrap::<bool>(true).0;
    unsafe {
        printf(c"mod %u %d\n".as_ptr(), ch, wb as i32);
    }

    let code = nth(3, 11u8, 42u8) as i32 + pick(false, 0, 1);
    unsafe {
        exit(code);
    }
}
