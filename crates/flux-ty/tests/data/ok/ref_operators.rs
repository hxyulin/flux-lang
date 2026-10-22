// Operators on references to integers and `bool`: `core`'s impls for
// `&i32 + i32`, `i32 + &i32`, `&i32 + &i32`, `i32 += &i32`, `-&i32` and
// `!&bool` give the referent's type, and an integer literal on the other
// side takes it too. Shifts take any integer type, by value or by
// reference. `==` compares what references point to, and so do `<` and
// the other comparisons, whichever references are mutable; bindings that
// match ergonomics makes references work like the values. Generic code
// bounded by the operator traits accepts references.

use core::ops::{Add, Neg};

fn arithmetic(r: &i32, s: &i32, x: i32) -> i32 {
    let a = r + 1;
    let b = 2 * s;
    let c = r - s;
    let d = x / r;
    let e = r % 3 + s & 7;
    let f = r | x ^ s;
    a + b + c + d + e + f
}

fn literals(r: &u8) -> u8 {
    let a = r + 1;
    let b = 200 - r;
    let c = &5 * r;
    a + b + c
}

fn unary(r: &i64, b: &bool) -> (i64, i64, bool) {
    (-r, !r, !b)
}

fn shifts(r: &u8, n: &u32, x: u64) -> u64 {
    let a = r << 2;
    let b = x >> n;
    let c = r << n;
    let d = 1u64 << r;
    let mut e = x;
    e <<= r;
    e >>= 1u8;
    a as u64 + b + c as u64 + d + e
}

fn compound(r: &i16, b: &bool) -> (i16, bool) {
    let mut x = 10;
    x += r;
    x -= &3;
    x *= r;
    x /= &2;
    x %= r;
    x &= r;
    x |= &1;
    x ^= r;
    let mut y = true;
    y &= b;
    y ^= &false;
    (x, y)
}

fn comparisons(a: &i32, b: &i32, m: &mut i32) -> bool {
    let eq = a == b && a != m && m == a;
    let ord = a < b || a <= &3 || b > a || m >= &mut 2;
    let flags = &true == &false || &true < &false;
    eq || ord || flags
}

fn ergonomics(pair: &(i32, u8), o: &Option<i32>) -> i32 {
    let (a, b) = pair;
    let sum = match o {
        Some(x) if x > &0 => x + a,
        Some(x) => -x,
        None => 0,
    };
    sum + (b << 1) as i32
}

fn add<T: Add<i32, Output = i32>>(t: T) -> i32 {
    t + 1
}

fn negate<T: Neg<Output = i32>>(t: T) -> i32 {
    -t
}

fn same<T: PartialEq>(a: T, b: T) -> bool {
    a == b
}

fn generic(r: &i32) -> bool {
    let n = add(r) + negate(r);
    same(r, &n)
}

fn main() {
    let mut m = 3;
    arithmetic(&1, &2, 3);
    literals(&4);
    unary(&5, &true);
    shifts(&1, &2, 3);
    compound(&3, &true);
    comparisons(&1, &2, &mut m);
    ergonomics(&(1, 2), &Some(3));
    generic(&4);
}
