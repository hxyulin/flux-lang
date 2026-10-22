// `impl Trait` parameters are anonymous type parameters; return types
// are opaque types, known to callers by their bounds only, whose hidden
// types the bodies decide.

fn evens(limit: u64) -> impl Iterator<Item = u64> {
    (0..limit).filter(|n| n % 2 == 0)
}

fn doubled<I: Iterator<Item = u32>>(it: I) -> impl Iterator<Item = u32> {
    it.map(|x| x * 2)
}

fn call_twice(f: impl Fn(u32) -> u32, x: u32) -> u32 {
    f(f(x))
}

fn compose(f: impl Fn(i32) -> i32, g: impl Fn(i32) -> i32) -> impl Fn(i32) -> i32 {
    move |x| g(f(x))
}

fn maybe(c: bool) -> Option<impl Fn() -> u32> {
    if c { Some(|| 7) } else { None }
}

fn copyable(x: u8) -> impl Copy + Clone {
    x
}

fn countdown(n: u32) -> impl Copy {
    if n == 0 { 0 } else { countdown(n - 1) }
}

fn first<T: Copy>(t: T, explicit: impl Copy) -> T {
    let _ = explicit;
    t
}

fn sum_all<I: Iterator<Item = u64>>(it: I) -> u64 {
    it.sum()
}

struct Counter {
    step: u32,
}

impl Counter {
    fn steps(&self, n: u32) -> impl Iterator<Item = u32> + '_ {
        (0..n).map(move |i| i * self.step)
    }
}

fn main() {
    let a: u64 = evens(10).map(|x| x + 1).take(2).sum();
    let mut it = evens(4);
    let b: Option<u64> = it.next();
    for x in evens(3) {
        let _: u64 = x;
    }
    let c: u32 = doubled(0..3u32).sum();
    let d = call_twice(|x| x * 3, 2);
    let e = compose(|x| x + 1, |x| x * 10)(4);
    let f = maybe(true).map(|g| g());
    let k = copyable(9);
    let k2 = k;
    let k3 = k.clone();
    let r = countdown(3);
    let s = sum_all(evens(5));
    let t: u32 = Counter { step: 3 }.steps(4).sum();
    let u = first::<u8>(1, 2u16);
    let _ = (a, b, c, d, e, f, k2, k3, r, s, t, u);
}
