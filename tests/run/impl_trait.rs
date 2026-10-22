//! `impl Trait`: iterator pipelines returned as `impl Iterator`, closures
//! returned as `impl Fn`, `impl Trait` parameters taking closures and
//! iterators, opaque types that borrow (`+ '_`) and that are generic,
//! and hidden types with drop glue.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

fn evens(limit: u64) -> impl Iterator<Item = u64> {
    (0..limit).filter(|n| n % 2 == 0)
}

fn squares_of<I: Iterator<Item = u64>>(it: I) -> impl Iterator<Item = u64> {
    it.map(|x| x * x)
}

fn adder(n: i64) -> impl Fn(i64) -> i64 {
    move |x| x + n
}

fn counter() -> impl FnMut() -> i64 {
    let mut count = 0;
    move || {
        count += 1;
        count
    }
}

fn compose(f: impl Fn(i64) -> i64, g: impl Fn(i64) -> i64) -> impl Fn(i64) -> i64 {
    move |x| g(f(x))
}

fn apply_n(mut f: impl FnMut() -> i64, n: u32) -> i64 {
    let mut last = 0;
    for _ in 0..n {
        last = f();
    }
    last
}

fn total(it: impl Iterator<Item = u64>) -> u64 {
    let mut sum = 0;
    for x in it {
        sum += x;
    }
    sum
}

fn pick(big: bool) -> impl Fn(i64) -> i64 {
    let scale = if big { 100 } else { 2 };
    move |x| x * scale
}

fn repeat_val<T: Copy>(v: T, n: usize) -> impl Iterator<Item = T> {
    (0..n).map(move |_| v)
}

fn countdown(n: i64) -> impl Fn(i64) -> i64 {
    if n == 0 { adder(0) } else { countdown(n - 1) }
}

struct Ring {
    items: [i64; 6],
    len: usize,
    start: usize,
}

impl Ring {
    fn iter(&self) -> impl Iterator<Item = i64> + '_ {
        (0..self.len).map(move |i| self.items[(self.start + i) % self.items.len()])
    }

    fn push(&mut self, x: i64) {
        let at = (self.start + self.len) % self.items.len();
        self.items[at] = x;
        if self.len < self.items.len() {
            self.len += 1;
        } else {
            self.start = (self.start + 1) % self.items.len();
        }
    }
}

struct Noisy {
    id: i64,
}

impl Drop for Noisy {
    fn drop(&mut self) {
        show(99, self.id);
    }
}

fn guarded(id: i64) -> impl Fn() -> i64 {
    let noisy = Noisy { id };
    move || {
        let whole = &noisy;
        whole.id * 2
    }
}

fn main() {
    show(1, total(evens(10)) as i64);
    let sq: u64 = squares_of(evens(7)).sum();
    show(2, sq as i64);
    let add5 = adder(5);
    show(3, add5(1) + add5(2));
    let mut next = counter();
    next();
    show(4, next());
    show(5, apply_n(counter(), 7));
    let f = compose(adder(1), |x| x * 3);
    show(6, f(4));
    show(7, pick(true)(3) + pick(false)(3));
    let r: i32 = repeat_val(4, 5).sum();
    show(8, r as i64);
    show(9, countdown(3)(9));
    let mut ring = Ring {
        items: [0; 6],
        len: 0,
        start: 0,
    };
    for x in 1..=8 {
        ring.push(x);
    }
    let sum: i64 = ring.iter().sum();
    show(10, sum);
    let max = ring.iter().max().unwrap_or(-1);
    ring.push(20);
    show(11, max);
    show(12, ring.iter().filter(|x| x % 2 == 0).count() as i64);
    let g = guarded(21);
    show(13, g());
    show(14, evens(100).skip(3).step_by(4).take(3).sum::<u64>() as i64);
}
