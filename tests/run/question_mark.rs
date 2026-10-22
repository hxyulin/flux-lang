//! `?` on `Option` and `Result`: the value on success, an early return of
//! `None` or of the error otherwise, converted with `From` (the
//! program's impls and `core`'s integer widenings); several in one
//! expression, in loops and in conditions; and the values in scope,
//! dropped in rustc's order when `?` returns. The exit status is a
//! checksum of the results.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
    fn exit(code: i32) -> !;
}

fn show(label: *const i8, v: i64) {
    unsafe {
        printf(c"%s %lld\n".as_ptr(), label, v);
    }
}

struct Noisy(i64);

impl Drop for Noisy {
    fn drop(&mut self) {
        show(c"drop".as_ptr(), self.0);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ParseError {
    Empty,
    Negative(i64),
}

struct AppError {
    code: i64,
}

impl From<ParseError> for AppError {
    fn from(e: ParseError) -> AppError {
        show(c"convert".as_ptr(), 0);
        match e {
            ParseError::Empty => AppError { code: 100 },
            ParseError::Negative(n) => AppError { code: 200 - n },
        }
    }
}

fn half(n: i64) -> Option<i64> {
    if n % 2 == 0 { Some(n / 2) } else { None }
}

fn quarter(n: i64) -> Option<i64> {
    let h = half(n)?;
    Some(half(h)? + 0)
}

fn parse(n: i64) -> Result<i64, ParseError> {
    if n == 0 {
        Err(ParseError::Empty)
    } else if n < 0 {
        Err(ParseError::Negative(n))
    } else {
        Ok(n * 10)
    }
}

fn sum_parsed(a: i64, b: i64) -> Result<i64, ParseError> {
    Ok(parse(a)? + parse(b)?)
}

fn app(a: i64) -> Result<i64, AppError> {
    let _guard = Noisy(a);
    let v = parse(a)?;
    let _other = Noisy(v);
    Ok(v + 1)
}

fn widen(x: Result<u8, u8>) -> Result<u32, u16> {
    let v = x?;
    Ok(v as u32 * 1000)
}

fn first_odd(limit: i64) -> Option<i64> {
    let mut i = 0;
    loop {
        if i >= limit {
            return None;
        }
        if half(i).is_none() {
            return Some(i);
        }
        i += half(i)? * 0 + 1;
    }
}

fn all_even(values: (i64, i64, i64)) -> Option<i64> {
    let (a, b, c) = values;
    if half(a)? > 0 && half(b)? > 0 {
        return Some(half(c)?);
    }
    Some(-1)
}

fn record(total: &mut i64, label: *const i8, v: i64) {
    show(label, v);
    *total = *total * 31 + v;
}

// Each arm prints, which keeps keel's optimizer from turning these
// `match`es into a `switch` that passes the value to the return block:
// its arm64 code generation loses the jump to the default target then.
fn option_code(o: Option<i64>) -> i64 {
    match o {
        Some(v) => {
            show(c"some".as_ptr(), v);
            v
        }
        None => {
            show(c"none".as_ptr(), 0);
            -1
        }
    }
}

fn result_code(r: Result<i64, ParseError>) -> i64 {
    match r {
        Ok(v) => {
            show(c"ok".as_ptr(), v);
            v
        }
        Err(ParseError::Empty) => -2,
        Err(ParseError::Negative(n)) => n * 100,
    }
}

fn app_code(r: Result<i64, AppError>) -> i64 {
    match r {
        Ok(v) => {
            show(c"ok".as_ptr(), v);
            v
        }
        Err(e) => -e.code,
    }
}

fn main() {
    let mut total = 0;
    record(&mut total, c"quarter8".as_ptr(), option_code(quarter(8)));
    record(&mut total, c"quarter6".as_ptr(), option_code(quarter(6)));
    record(&mut total, c"quarter5".as_ptr(), option_code(quarter(5)));
    record(&mut total, c"sum".as_ptr(), result_code(sum_parsed(2, 3)));
    record(&mut total, c"sum_empty".as_ptr(), result_code(sum_parsed(2, 0)));
    record(&mut total, c"sum_neg".as_ptr(), result_code(sum_parsed(-4, 0)));
    record(&mut total, c"app".as_ptr(), app_code(app(7)));
    record(&mut total, c"app_empty".as_ptr(), app_code(app(0)));
    record(&mut total, c"app_neg".as_ptr(), app_code(app(-3)));
    let widened = match widen(Ok(7)) {
        Ok(v) => v as i64,
        Err(e) => -(e as i64),
    };
    record(&mut total, c"widen".as_ptr(), widened);
    let widened = match widen(Err(42)) {
        Ok(v) => v as i64,
        Err(e) => -(e as i64),
    };
    record(&mut total, c"widen_err".as_ptr(), widened);
    record(&mut total, c"first_odd".as_ptr(), option_code(first_odd(10)));
    record(&mut total, c"all_even".as_ptr(), option_code(all_even((2, 4, 8))));
    record(&mut total, c"all_even_c".as_ptr(), option_code(all_even((2, 4, 7))));
    record(&mut total, c"all_even_a".as_ptr(), option_code(all_even((3, 4, 8))));
    show(c"total".as_ptr(), total);
    unsafe { exit((total & 0x7f) as i32) }
}
