//! Operators on references to integers and `bool`: arithmetic, bitwise
//! and shift operators with a reference on either side or both, compound
//! assignments with a reference on the right, `-` and `!` of references,
//! comparisons of references (shared, mutable and mixed), the bindings
//! match ergonomics makes references, and generic code bounded by the
//! operator traits called with references. The exit status is a
//! checksum of the results.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
    fn exit(code: i32) -> !;
}

use core::ops::{Add, Shl};

fn show(label: *const i8, v: i64) {
    unsafe {
        printf(c"%s %lld\n".as_ptr(), label, v);
    }
}

fn arith(r: &i32, s: &i32, x: i32) -> i32 {
    let a = r + 1;
    let b = 2 * s;
    let c = r - s;
    let d = r * s + x;
    let e = r / s;
    let f = r % s;
    let g = r & s;
    let h = r | x;
    let i = x ^ s;
    let j = -r;
    let k = !s;
    a + b + c + d + e + f + g + h + i + j + k
}

fn wrapping(r: &u8, s: &i8) -> i64 {
    let a = r + 200;
    let b = s * &100;
    let c = -s;
    (a as i64) * 1000 + (b as i64) * 10 + c as i64
}

fn shifts(r: &u8, n: &u32, m: u64) -> u64 {
    let a = r << 2;
    let b = r >> n;
    let c = m << n;
    let d = 1u64 << r;
    let e = r << 9i64;
    let mut f = m;
    f <<= r;
    f >>= 1u8;
    f >>= &2i128;
    a as u64 + b as u64 + c + d + e as u64 + f
}

fn compound(r: &i64) -> i64 {
    let mut x = 10i64;
    x += r;
    x -= &3;
    x *= r;
    x /= &2;
    x %= r;
    x |= r;
    x &= &0xff;
    x ^= r;
    x <<= r;
    x >>= &1;
    x
}

fn compare(a: &i32, b: &i32, c: &mut i32) -> i64 {
    let mut n = 0;
    if a == b {
        n += 1;
    }
    if a != b {
        n += 2;
    }
    if a < b {
        n += 4;
    }
    if a >= b {
        n += 8;
    }
    if a == c {
        n += 16;
    }
    if a <= c {
        n += 32;
    }
    *c += 1;
    if c == c && c > &mut 0 {
        n += 64;
    }
    if &true > &false && &1 < &2 {
        n += 128;
    }
    n
}

fn bools(p: &bool, q: bool) -> i64 {
    let a = p & q;
    let b = q | p;
    let c = p ^ p;
    let d = !p;
    let mut e = q;
    e ^= p;
    e |= &true;
    (a as i64) + (b as i64) * 2 + (c as i64) * 4 + (d as i64) * 8 + (e as i64) * 16
}

fn pairs(pair: &(i32, i32)) -> i32 {
    match pair {
        (a, b) if a < b => a + b,
        (a, b) if a == b => -a,
        (a, b) => a * b - 1,
    }
}

fn option(o: &Option<u16>) -> u16 {
    match o {
        Some(x) if x % 2 == 0 => x / 2,
        Some(x) => x * 3 + 1,
        None => 0,
    }
}

fn record(total: &mut i64, label: *const i8, v: i64) {
    show(label, v);
    *total += v;
}

fn add_one<T: Add<i32, Output = i32>>(t: T) -> i32 {
    t + 1
}

fn double<T: Shl<u8, Output = u32>>(t: T) -> u32 {
    t << 1
}

fn same<T: PartialEq>(a: T, b: T) -> bool {
    a == b
}

fn main() {
    let mut total: i64 = 0;
    record(&mut total, c"arith".as_ptr(), arith(&7, &3, 5) as i64);
    record(&mut total, c"arith_neg".as_ptr(), arith(&-9, &4, -2) as i64);
    record(&mut total, c"wrapping".as_ptr(), wrapping(&100, &-128));
    record(&mut total, c"shifts".as_ptr(), shifts(&3, &4, 100) as i64);
    record(&mut total, c"compound".as_ptr(), compound(&5));
    let mut c = 4;
    record(&mut total, c"compare".as_ptr(), compare(&4, &9, &mut c));
    record(&mut total, c"compare2".as_ptr(), compare(&9, &4, &mut c));
    record(&mut total, c"bools".as_ptr(), bools(&true, false));
    record(&mut total, c"pairs".as_ptr(), pairs(&(2, 5)) as i64);
    record(&mut total, c"pairs2".as_ptr(), pairs(&(6, 5)) as i64);
    record(&mut total, c"pairs3".as_ptr(), pairs(&(7, 7)) as i64);
    record(&mut total, c"option".as_ptr(), option(&Some(6)) as i64);
    record(&mut total, c"option2".as_ptr(), option(&Some(7)) as i64);
    record(&mut total, c"generic".as_ptr(), add_one(&41) as i64);
    record(&mut total, c"double".as_ptr(), double(&21u32) as i64);
    record(&mut total, c"same".as_ptr(), same(&3, &3) as i64 + same(&3, &4) as i64);
    show(c"total".as_ptr(), total);
    unsafe { exit((total & 0x7f) as i32) }
}
