//! `core::iter`'s sources (`empty`, `once`, `once_with`, `repeat`,
//! `repeat_with`, `repeat_n`, `successors`, `from_fn`), with their sizes,
//! back ends and drops; `bool::then_some` and `bool::then`; and
//! `core::cmp::Reverse`, ordering the other way round.

use core::cmp::Reverse;
use core::iter;

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

fn hint(label: i32, h: (usize, Option<usize>)) {
    let upper = match h.1 {
        Some(n) => n as i64,
        None => -1,
    };
    unsafe {
        printf(c"%d: hint %llu %lld\n".as_ptr(), label, h.0 as u64, upper);
    }
}

struct Loud(i32);

impl Clone for Loud {
    fn clone(&self) -> Loud {
        show(800, self.0 as i64);
        Loud(self.0 + 1)
    }
}

impl Drop for Loud {
    fn drop(&mut self) {
        show(900, self.0 as i64);
    }
}

trait Pick {
    fn pick(self) -> i32;
}

impl Pick for usize {
    fn pick(self) -> i32 {
        self as i32 * 2
    }
}

fn call_twice<F: FnMut(i32) -> i32>(f: &mut F) -> i32 {
    f(1) + f(2)
}

fn main() {
    let mut e = iter::empty::<i32>();
    show(1, e.next().unwrap_or(-1) as i64);
    hint(2, e.size_hint());
    show(3, iter::once(5).chain(iter::once(6)).sum::<i32>() as i64);
    let mut o = iter::once(7);
    hint(4, o.size_hint());
    show(5, o.next_back().unwrap_or(0) as i64);
    hint(6, o.size_hint());
    show(7, iter::repeat(3).take(4).sum::<i32>() as i64);
    hint(8, iter::repeat(1u8).size_hint());
    let powers: i64 = iter::successors(Some(1i64), |&x| if x < 1000 { Some(x * 3) } else { None }).sum();
    show(9, powers);
    let mut n = 0;
    let counted = iter::from_fn(|| {
        n += 1;
        if n <= 5 { Some(n) } else { None }
    });
    show(10, counted.map(|x| x * x).sum::<i32>() as i64);
    {
        let mut r = iter::repeat_n(Loud(10), 3);
        hint(11, r.size_hint());
        show(12, r.len() as i64);
        let a = r.next();
        show(13, a.as_ref().map_or(-1, |l| l.0) as i64);
        let b = r.next_back();
        show(14, b.as_ref().map_or(-1, |l| l.0) as i64);
        let c = r.next();
        show(15, c.as_ref().map_or(-1, |l| l.0) as i64);
        show(16, r.next().is_none() as i64);
    }
    {
        let z = iter::repeat_n(Loud(20), 0);
        show(17, z.len() as i64);
    }
    {
        let r = iter::repeat_n(Loud(30), 2);
        show(18, r.len() as i64);
    }
    let mut k = 0;
    let mut evens = iter::repeat_with(|| {
        k += 2;
        k
    });
    show(19, evens.next().unwrap_or(0) as i64 + evens.next().unwrap_or(0) as i64);
    let w = iter::once_with(|| 41).map(|x| x + 1).next().unwrap_or(0);
    show(20, w as i64);
    show(21, 3.pick() as i64);
    let mut add = |x: i32| x + 10;
    show(22, call_twice(&mut add) as i64);
    show(24, (3 > 2).then_some(4).unwrap_or(0) as i64);
    show(25, (2 > 3).then_some(4).unwrap_or(0) as i64);
    let mut calls = 0;
    let lazy = (2 > 3).then(|| {
        calls += 1;
        5
    });
    show(26, lazy.unwrap_or(-1) as i64 + calls as i64 * 100);
    let eager = true.then(|| {
        calls += 1;
        6
    });
    show(27, eager.unwrap_or(-1) as i64 + calls as i64 * 100);
    let mut xs = [3, 9, 1, 7];
    xs.sort_by_key(|&x| Reverse(x));
    for x in xs {
        show(28, x as i64);
    }
    let mut pairs = [(2, 'b'), (1, 'a'), (2, 'a'), (1, 'c')];
    pairs.sort_by_key(|&(n, c)| (Reverse(n), c));
    for (n, c) in pairs {
        show(29, n as i64 * 1000 + c as i64);
    }
    show(30, (Reverse(1) < Reverse(2)) as i64);
    show(31, (Reverse(1) > Reverse(2)) as i64);
    show(32, (Reverse(3) == Reverse(3)) as i64);
    show(33, Reverse(5).max(Reverse(8)).0 as i64);
    show(34, (Reverse('a') <= Reverse('a')) as i64);
    let r = Reverse(7);
    let copied = r;
    show(35, (r.0 + copied.clone().0) as i64);
    show(36, Reverse::<u8>::default().0 as i64);
    let fib = iter::successors(Some((0u64, 1u64)), |&(a, b)| Some((b, a + b))).map(|p| p.0).take(10);
    show(23, fib.last().unwrap_or(0) as i64);
}
