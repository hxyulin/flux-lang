// Generic functions, structs and impl blocks: inference of their arguments.

fn id<T>(x: T) -> T {
    x
}

fn pick<T>(first: bool, a: T, b: T) -> T {
    if first { a } else { b }
}

fn pair<A, B>(a: A, b: B) -> (A, B) {
    (a, b)
}

fn make<T>(x: T, y: T) -> Pair<T> {
    Pair::new(x, id(x2(y)))
}

fn x2<T>(x: T) -> T {
    x
}

// A parameter is inferred from the arguments, the expected type or a turbofish.
fn infer() -> u8 {
    let a = id(1u8);
    let b: i64 = id(2);
    let c = id::<i16>(3);
    let d = pick(true, 1, 2u32);
    let (e, f) = pair(true, 1u16);
    let g = pair::<_, u64>(1i8, 5);
    let _ = (b, c, d, e, f, g);
    a
}

struct Pair<T> {
    a: T,
    b: T,
}

struct Wrap<T>(T);

struct Unit<T> {
    inner: Wrap<T>,
}

struct Two<K, V> {
    key: K,
    value: V,
}

impl<T> Pair<T> {
    fn new(a: T, b: T) -> Self {
        Pair { a, b }
    }

    fn first(self) -> T {
        self.a
    }

    fn set_first(&mut self, a: T) {
        self.a = a;
    }

    fn convert<U>(self, a: U, b: U) -> Pair<U> {
        let _old = self;
        Pair { a, b }
    }

    fn with_other(self, other: Pair<T>) -> Pair<Pair<T>> {
        Pair::new(self, other)
    }
}

// Blocks for single instances; each instance has its own `sum`.
impl Pair<i32> {
    fn sum(&self) -> i32 {
        self.a + self.b
    }

    const ZERO: i32 = 0;
}

impl Pair<u8> {
    fn sum(&self) -> u8 {
        self.a + self.b
    }
}

impl<K, V> Two<K, V> {
    fn flip(self) -> Two<V, K> {
        Two {
            key: self.value,
            value: self.key,
        }
    }
}

// Struct literals, fields and methods of instances.
fn instances() -> i32 {
    let p: Pair<i32> = Pair::new(1, 2);
    let q: Pair<u8> = Pair::new(3, 4);
    let r = Pair::<i64>::new(5, 6);
    let s = Pair::<u16> { a: 7, b: 8 };
    let w = Wrap(true);
    let u = Unit { inner: Wrap(1u32) };
    let t = Two { key: 1u8, value: false }.flip();
    let big = p.sum() + q.sum() as i32 + Pair::<i32>::ZERO;
    let _ = (r.a, s.b, w.0, u.inner.0, t.key, t.value);
    big
}

// `_` in types inside bodies, inferred from the initializer or later uses.
fn placeholders() -> u64 {
    let p: Pair<_> = Pair::new(1u64, 2);
    let mut q: Pair<_> = Pair { a: 0, b: 0 };
    q.set_first(3i8);
    let w = id::<Wrap<_>>(Wrap(p.b));
    let _ = q;
    w.0
}

// Methods with their own parameters, and turbofish on methods.
fn methods() -> bool {
    let p = Pair::new(1, 2i32);
    let r = p.sum();
    let q = p.convert::<bool>(true, false);
    let c = Pair::new(1u8, 2).convert(r, 4);
    let n = Pair::new(1, 2).with_other(Pair::new(3, 4));
    let _ = (c.a, n.a.a, n.b.first());
    q.first()
}

// A generic call inside a generic function.
fn nested<T>(x: T, y: T) -> Pair<T> {
    make(x, y)
}

fn main() {
    let _ = infer();
    let _ = instances();
    let _ = placeholders();
    let _ = methods();
    let _ = nested(1, 2).a;
}
