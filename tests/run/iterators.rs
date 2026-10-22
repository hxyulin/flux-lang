//! `for` loops over ranges, iterators of the program's own and adapters,
//! with labels, `break` and `continue`, and the drop of the iterator when
//! the loop ends; ranges of other integer types and `contains`; and the
//! adapters and consumers of `Iterator` with closures.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

struct Noisy(i32);

impl Drop for Noisy {
    fn drop(&mut self) {
        show(100, self.0 as i64);
    }
}

struct Countdown {
    n: u32,
    _guard: Noisy,
}

impl Iterator for Countdown {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.n == 0 {
            None
        } else {
            self.n -= 1;
            Some(self.n)
        }
    }
}

fn main() {
    let mut total = 0;
    for i in 0..10 {
        total += i;
    }
    show(1, total);

    let mut t2: u8 = 0;
    for i in 1..=5u8 {
        t2 += i;
    }
    show(2, t2 as i64);

    let mut count = 0;
    'outer: for i in 0..10 {
        for j in 0..10 {
            if j > i {
                continue 'outer;
            }
            if i * j > 20 {
                break 'outer;
            }
            count += 1;
        }
    }
    show(3, count);

    for x in (Countdown { n: 3, _guard: Noisy(7) }) {
        show(4, x as i64);
    }

    let mut rev = 0i64;
    for (i, x) in (0..4).rev().enumerate() {
        rev = rev * 10 + (x * 2 + i as i64);
    }
    show(5, rev);

    let s: i32 = (1..=10).filter(|x| x % 2 == 0).map(|x| x * x).sum();
    show(6, s as i64);
    let c = (0..100).skip(10).step_by(7).take(5).count();
    show(7, c as i64);
    let k = 3;
    let found = (0..50).find(|&x| x * k > 40);
    show(8, found.unwrap_or(-1));
    show(9, (0..10).any(|x| x == 7) as i64 + (0..10).all(|x| x < 5) as i64 * 2);
    let m = (1..6).zip(10..20).map(|(a, b)| a * b).max().unwrap();
    show(10, m);
    let f = (1..=4).fold(1i64, |acc, x| acc * x);
    show(11, f);
    let p: u64 = (1..=10u64).product();
    show(12, p as i64);
    let pos = (5..20).position(|x| x % 7 == 0).unwrap();
    show(13, pos as i64);
    let last = (0..7).chain(20..23).last().unwrap();
    show(14, last);
    let mn = (3..9).map(|x: i32| (x - 5) * (x - 5)).min().unwrap();
    show(15, mn as i64);
    let mut n = 0;
    (0..5).for_each(|x| n += x);
    show(16, n);
    let tw: i32 = (1..).take_while(|&x| x * x < 50).sum();
    show(17, tw as i64);
    let sw = (1..10).skip_while(|&x| x < 7).count();
    show(18, sw as i64);
    let fm: i32 = (0..10).filter_map(|x| if x % 3 == 0 { Some(x * 10) } else { None }).sum();
    show(19, fm as i64);
    let r = 2..5;
    show(20, r.contains(&4) as i64 + (1..=3).contains(&3) as i64 * 2);
    let mut it = (10..13).map(|x| x + 1);
    while let Some(v) = it.next() {
        show(21, v);
    }
}
