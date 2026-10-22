// Closures: signatures from annotations, from the bounds of what they
// are passed to, or from their calls; calls of closures and of type
// parameters bounded by the `Fn`-family traits.

struct Holder<F> {
    f: F,
}

impl<F: Fn(i32) -> i32> Holder<F> {
    fn run(&self, x: i32) -> i32 {
        (self.f)(x)
    }
}

fn apply<F: Fn(i32) -> i32>(f: F, x: i32) -> i32 {
    f(x)
}

fn apply_mut<F>(mut f: F)
where
    F: FnMut(u8, bool),
{
    f(1, true);
}

fn consume<F: FnOnce() -> u64>(f: F) -> u64 {
    f()
}

fn by_ref<F: Fn(i64) -> i64>(f: &F) -> i64 {
    f(2)
}

fn generic<T: Copy>(t: T) -> (T, T) {
    let dup = |x: T| (x, x);
    dup(t)
}

fn inference() -> i32 {
    let k = 3;
    let add = |a, b| a + b;
    let sum = add(1, 2);
    let from_bound = apply(|x| x * k, 4);
    let mut seen = 0u8;
    apply_mut(|n, flag| {
        if flag {
            seen += n;
        }
    });
    let big = consume(move || 7);
    let twice = |x: i64| -> i64 { x * 2 };
    let through = by_ref(&twice);
    let (a, b) = generic(true);
    let h = Holder { f: |x| x - 1 };
    sum + from_bound + seen as i32 + big as i32 + through as i32 + h.run(1) + (a && b) as i32
}

fn iterators() -> i32 {
    let evens: i32 = (0..10).filter(|x| x % 2 == 0).map(|x| x * 3).sum();
    let count = (1..=5).rev().skip(1).count();
    let found = (0u8..).find(|&x| x > 4);
    let pairs = (0..3).zip(5..8).fold(0, |acc, (a, b)| acc + a * b);
    let mut total = 0;
    for (i, x) in (10..13).enumerate() {
        total += i as i32 * x;
    }
    evens + count as i32 + found.unwrap_or(0) as i32 + pairs + total
}

fn options() -> i32 {
    let a = Some(2);
    let b = a.map(|x| x + 1).and_then(|x| if x > 2 { Some(x) } else { None });
    let r: Result<i32, u8> = Ok(1);
    b.unwrap_or_else(|| 0) + r.map_err(|e| e as i32).unwrap_or(0)
}

fn main() {}
