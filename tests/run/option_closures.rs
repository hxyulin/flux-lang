//! `Option` and `Result` methods that take closures, with values moved
//! into closures that run or are dropped unrun.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i32) {
    unsafe {
        printf(c"%d: %d\n".as_ptr(), label, value);
    }
}

struct Noisy(i32);

impl Drop for Noisy {
    fn drop(&mut self) {
        show(100, self.0);
    }
}

fn parse(x: i32) -> Result<i32, u8> {
    if x >= 0 { Ok(x) } else { Err(1) }
}

fn half(x: i32) -> Option<i32> {
    if x % 2 == 0 { Some(x / 2) } else { None }
}

fn main() {
    let a = Some(4);
    show(1, a.map(|x| x * 3).unwrap_or(0));
    show(2, a.and_then(|x| half(x)).and_then(|x| half(x)).and_then(|x| half(x)).unwrap_or(-1));
    let offset = 10;
    show(3, None.map_or(offset, |x: i32| x + offset));
    show(4, a.map_or_else(|| -1, |x| x + 1));
    show(5, a.filter(|x| *x > 5).is_some() as i32);
    show(6, None.unwrap_or_else(|| offset * 2));
    show(7, a.ok_or_else(|| 3u8).unwrap_or(0));
    show(8, None.or_else(|| Some(9)).unwrap());
    show(9, a.is_some_and(|x| x == 4) as i32 + a.is_none_or(|x| x > 10) as i32 * 2);
    let (p, q) = a.zip(Some(5)).unwrap();
    show(10, p * q);
    let mut seen = 0;
    let b = a.inspect(|x| seen = *x);
    show(11, seen + b.unwrap());

    let r = parse(5);
    show(12, r.map(|x| x * 2).unwrap_or(0));
    show(13, parse(-1).map_err(|e| e as i32 * 7).unwrap_or_else(|e| e));
    show(14, r.and_then(|x| parse(x - 10)).is_err() as i32);
    show(15, parse(-3).or_else(|_| parse(3)).unwrap_or(0));
    show(16, parse(-3).map_or(42, |x| x));
    show(17, r.map_or_else(|e| e as i32, |x| x + 100));
    show(18, r.is_ok_and(|x| x > 2) as i32 + parse(-1).is_err_and(|e| e == 1) as i32);

    // A value moved into a closure that runs, or is dropped unrun.
    let n = Noisy(5);
    let used = Some(1).map(move |x| x + n.0);
    show(19, used.unwrap());
    let m = Noisy(6);
    let unused = None.map(move |x: i32| x + m.0);
    show(20, unused.is_none() as i32);
}
