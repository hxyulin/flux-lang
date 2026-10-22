//! `CStr`: comparisons (`==`, `<`, `cmp`, by reference too), its bytes
//! with and without the NUL, its length, and C strings made from bytes,
//! with `from_bytes_with_nul`'s and `from_bytes_until_nul`'s errors.

use core::cmp::Ordering;
use core::ffi::{CStr, FromBytesWithNulError};

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

fn order(a: &CStr, b: &CStr) -> i64 {
    match a.cmp(b) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

fn name(n: i32) -> &'static CStr {
    match n {
        0 => c"zero",
        1 => c"one",
        _ => c"many",
    }
}

fn sum(bytes: &[u8]) -> i64 {
    let mut total = 0;
    for &b in bytes {
        total += b as i64;
    }
    total
}

fn main() {
    let a = name(1);
    let b = c"one";
    show(1, (a == b) as i64 + 2 * (a != name(0)) as i64);
    show(2, (name(0) < name(2)) as i64 + 2 * (name(2) > name(1)) as i64);
    show(3, order(a, b) + 10 * order(name(0), name(1)) + 100 * order(c"ab", c"a"));
    show(4, (*a == b) as i64 + 2 * (a.max(name(0)) == c"zero") as i64);
    show(5, a.count_bytes() as i64 * 10 + c"".count_bytes() as i64);
    show(6, a.is_empty() as i64 + 2 * c"".is_empty() as i64);
    show(7, sum(a.to_bytes()) * 10 + a.to_bytes_with_nul().len() as i64);
    show(8, (a.to_bytes() == b"one") as i64 + 2 * (a.to_bytes_with_nul() == b"one\0") as i64);

    let good = [b'h', b'i', 0];
    let made = CStr::from_bytes_with_nul(&good).unwrap();
    show(9, (made == c"hi") as i64 + 2 * made.count_bytes() as i64);
    unsafe {
        printf(c"%s\n".as_ptr(), made.as_ptr());
    }
    let inner = [b'a', 0, b'b', 0];
    match CStr::from_bytes_with_nul(&inner) {
        Err(FromBytesWithNulError::InteriorNul { position }) => show(10, position as i64),
        _ => show(10, -1),
    }
    let open = [b'a', b'b'];
    let e = CStr::from_bytes_with_nul(&open);
    show(11, (e == Err(FromBytesWithNulError::NotNulTerminated)) as i64);
    let until = CStr::from_bytes_until_nul(&inner).unwrap();
    show(12, (until == c"a") as i64 + 2 * CStr::from_bytes_until_nul(&open).is_err() as i64);
    let text = c"flux".to_str().unwrap();
    show(13, text.len() as i64 + 10 * (text == "flux") as i64);
    let d: &CStr = Default::default();
    show(14, d.is_empty() as i64);
    let names = [c"b", c"c", c"a"];
    let mut least = names[0];
    for n in names {
        if n < least {
            least = n;
        }
    }
    show(15, (least == c"a") as i64);
}
