// `impl Trait` in a function's parameters becomes an anonymous type
// parameter, after the others; in its return type, an opaque type.

struct S {
    n: i32,
}

impl S {
    fn adder(&self) -> impl Fn(i32) -> i32 + '_ {
        move |x| x + self.n
    }

    fn apply<T: Copy>(&self, t: T, f: impl Fn(T) -> i32) -> i32 {
        f(t) + self.n
    }
}

fn apply(f: impl Fn(i32) -> i32, x: i32) -> i32 {
    f(x)
}

fn both(a: impl Copy, b: &impl Copy) -> (impl Copy, impl Copy) {
    (a, *b)
}

fn nested(it: impl Iterator<Item = impl Copy>) -> usize {
    it.count()
}

fn evens(limit: u32) -> impl Iterator<Item = u32> {
    (0..limit).filter(|n| n % 2 == 0)
}

fn captures<'a>(x: &'a u32, _y: &u32) -> impl Copy + use<'a> {
    *x
}

fn main() {
    let s = S { n: 1 };
    let _ = s.adder()(2) + s.apply(3, |x| x) + apply(|x| x + 1, 2);
    let _ = both(1, &2);
    let _ = nested(evens(5));
    let _ = captures(&1, &2);
}
