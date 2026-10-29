//! `match` on enums with `#[repr(i128)]` and `#[repr(u128)]`, whose
//! discriminants lie beyond 64 bits, including enums with fields, and on
//! 128-bit integers.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(n: i64) {
    unsafe {
        printf(c"%lld\n".as_ptr(), n);
    }
}

#[derive(Clone, Copy)]
#[repr(i128)]
enum Huge {
    Small = -170141183460469231731687303715884105728,
    BelowI64 = -9223372036854775809,
    Zero = 0,
    AboveI64 = 9223372036854775808,
    Big = 170141183460469231731687303715884105727,
}

#[derive(Clone, Copy)]
#[repr(u128)]
enum Unsigned {
    Low = 1,
    AboveU64 = 18446744073709551616,
    Top = 170141183460469231731687303715884105727,
}

#[derive(Clone, Copy)]
#[repr(i128)]
enum Data {
    Pair(i64, i64) = -9223372036854775809,
    One(i64) = 9223372036854775808,
    Nothing = 3,
}

#[inline(never)]
fn huge(h: Huge) -> i64 {
    match h {
        Huge::Small => 1,
        Huge::BelowI64 => 2,
        Huge::Zero => 3,
        Huge::AboveI64 => 4,
        Huge::Big => 5,
    }
}

#[inline(never)]
fn unsigned(u: Unsigned) -> i64 {
    match u {
        Unsigned::Low => 10,
        Unsigned::AboveU64 => 20,
        Unsigned::Top => 30,
    }
}

#[inline(never)]
fn data(d: Data) -> i64 {
    match d {
        Data::Pair(a, b) => a + b,
        Data::One(a) => a * 100,
        Data::Nothing => -1,
    }
}

#[inline(never)]
fn above(h: &Huge) -> bool {
    matches!(h, Huge::AboveI64 | Huge::Big)
}

#[inline(never)]
fn wide(n: u128) -> i64 {
    match n {
        0 => 1,
        18446744073709551616 => 2,
        340282366920938463463374607431768211455 => 3,
        _ => 4,
    }
}

#[inline(never)]
fn signed(n: i128) -> i64 {
    match n {
        -170141183460469231731687303715884105728 => 1,
        -1 => 2,
        9223372036854775807 => 3,
        9223372036854775808 => 4,
        _ => 5,
    }
}

fn main() {
    let all = [
        Huge::Small,
        Huge::BelowI64,
        Huge::Zero,
        Huge::AboveI64,
        Huge::Big,
    ];
    for h in all {
        show(huge(h));
        show(above(&h) as i64);
        show((h as i128 >> 64) as i64);
    }
    for u in [Unsigned::Low, Unsigned::AboveU64, Unsigned::Top] {
        show(unsigned(u));
        show((u as u128 >> 64) as i64);
    }
    for d in [Data::Pair(3, 4), Data::One(7), Data::Nothing] {
        show(data(d));
    }
    for n in [0, 1 << 64, u128::MAX, (1 << 64) - 1, 1] {
        show(wide(n));
    }
    for n in [i128::MIN, -1, i64::MAX as i128, 1 << 63, u64::MAX as i128, 0] {
        show(signed(n));
    }
}
