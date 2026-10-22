//! `Option` and `Result` from the prelude: their methods, comparisons and
//! nesting; `unwrap` on `None` panics, which aborts.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(n: i64) {
    unsafe {
        printf(c"%lld\n".as_ptr(), n);
    }
}

fn div(a: i64, b: i64) -> Result<i64, i64> {
    if b == 0 { Err(a) } else { Ok(a / b) }
}

fn find(limit: i64) -> Option<i64> {
    let mut i = 0;
    while i < limit {
        if i * i > 20 {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct P(i64);

// `unwrap` is a `const fn`, as in rustc.
const UNWRAPPED: i64 = Some(41).unwrap() + 1;

fn main() {
    show(UNWRAPPED);
    show(find(10).unwrap());
    show(find(3).unwrap_or(-1));
    show(find(3).is_none() as i64);
    match div(10, 2) {
        Ok(v) => show(v),
        Err(e) => show(-e),
    }
    match div(10, 0) {
        Ok(v) => show(v),
        Err(e) => show(-e),
    }
    show(div(7, 0).ok().unwrap_or(99));
    show(div(7, 7).is_ok() as i64);
    let a = Some(P(3));
    let b = a;
    show((a == b) as i64);
    show((a == None) as i64);
    let o: Option<i64> = Some(5);
    let r = o.ok_or(0);
    if let Ok(x) = r {
        show(x);
    }
    let nested: Option<Option<i64>> = Some(None);
    match nested {
        Some(Some(x)) => show(x),
        Some(None) => show(-2),
        None => show(-3),
    }
    let n: Option<i64> = None;
    show(n.unwrap());
}
