//! `Iterator::try_fold`, `try_for_each` and `DoubleEndedIterator::
//! try_rfold` with `Option`, `Result` and `ControlFlow` (whose methods are
//! here too), stopping at the first `None`, `Err` or `Break` and leaving
//! the iterator after it; `?` on a `ControlFlow`; and defaults of enums'
//! and structs' type parameters (`ControlFlow<B>`, `W<'a>`).

use core::ops::ControlFlow;

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

fn parse(s: &str) -> Option<i32> {
    s.bytes().try_fold(0i32, |n, b| {
        if b.is_ascii_digit() { Some(n * 10 + (b - b'0') as i32) } else { None }
    })
}

#[derive(Debug, PartialEq)]
struct TooBig(i32);

#[derive(Debug, PartialEq)]
struct Wide(i64);

impl From<TooBig> for Wide {
    fn from(t: TooBig) -> Wide {
        Wide(t.0 as i64 * 1000)
    }
}

fn sum_small(xs: &[i32]) -> Result<i32, TooBig> {
    xs.iter().try_fold(0, |acc, &x| if x < 10 { Ok(acc + x) } else { Err(TooBig(x)) })
}

fn widened(xs: &[i32]) -> Result<i64, Wide> {
    Ok(sum_small(xs)? as i64)
}

fn first_big(xs: &[i32]) -> ControlFlow<i32> {
    xs.iter().try_for_each(|&x| if x > 5 { ControlFlow::Break(x) } else { ControlFlow::Continue(()) })
}

fn then_three(xs: &[i32]) -> ControlFlow<i32, u8> {
    first_big(xs)?;
    ControlFlow::Continue(3)
}

fn flow_value(c: ControlFlow<i32, u8>) -> i64 {
    match c {
        ControlFlow::Continue(v) => v as i64,
        ControlFlow::Break(b) => -(b as i64),
    }
}

struct W<'a, T = i32>(&'a T);

fn get<'a>(w: W<'a>) -> &'a i32 {
    w.0
}

/// Counts the calls of `next`, to show where `try_fold` stopped.
struct Counted<'a> {
    calls: &'a mut i32,
    n: i32,
}

impl Iterator for Counted<'_> {
    type Item = i32;
    fn next(&mut self) -> Option<i32> {
        *self.calls += 1;
        self.n += 1;
        if self.n <= 10 { Some(self.n) } else { None }
    }
}

fn main() {
    show(1, parse("42").unwrap_or(-1) as i64);
    show(2, parse("4x2").unwrap_or(-1) as i64);
    show(3, parse("").unwrap_or(-1) as i64);
    show(4, match sum_small(&[1, 2, 3]) {
        Ok(v) => v as i64,
        Err(e) => -(e.0 as i64),
    });
    show(5, match sum_small(&[1, 20, 30]) {
        Ok(v) => v as i64,
        Err(e) => -(e.0 as i64),
    });
    show(6, match widened(&[4, 11]) {
        Ok(v) => v,
        Err(w) => w.0,
    });
    show(7, first_big(&[1, 7, 9]).break_value().unwrap_or(0) as i64);
    show(8, first_big(&[1, 2]).is_continue() as i64);
    show(9, first_big(&[8]).is_break() as i64);
    show(10, flow_value(then_three(&[1, 2])));
    show(11, flow_value(then_three(&[1, 8])));
    let total = (1..5).rev().try_rfold(0i32, |acc: i32, x| acc.checked_add(x));
    show(12, total.unwrap_or(-1) as i64);
    let wrapped = [i32::MAX, 1].iter().try_fold(0i32, |acc: i32, &x| acc.checked_add(x));
    show(13, wrapped.unwrap_or(-1) as i64);
    // The iterator is left after the item that stopped it.
    let mut calls = 0;
    let mut it = Counted { calls: &mut calls, n: 0 };
    let stopped = it.try_for_each(|x| if x == 4 { Err(x) } else { Ok(()) });
    show(14, stopped.unwrap_err() as i64);
    show(15, it.next().unwrap_or(0) as i64);
    show(16, calls as i64);
    let mut bytes = "abc".bytes();
    let r: Result<(), u8> = bytes.try_for_each(|b| if b == b'b' { Err(b) } else { Ok(()) });
    show(17, r.unwrap_err() as i64);
    show(18, bytes.next().unwrap_or(0) as i64);
    let mapped = first_big(&[9]).map_break(|b| b * 2).break_value().unwrap_or(0);
    show(19, mapped as i64);
    let cont: ControlFlow<i32, i32> = ControlFlow::Continue(5);
    show(20, cont.map_continue(|c| c + 1).continue_value().unwrap_or(0) as i64);
    let x = 31;
    show(21, *get(W(&x)) as i64);
    let none: Option<i32> = (0..0).try_fold(7, |a, b| Some(a + b));
    show(22, none.unwrap_or(-1) as i64);
}
