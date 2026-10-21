// Generic parameters on functions, structs and inherent `impl` blocks.

fn id<T>(x: T) -> T {
    x
}

fn first<A, B>(a: A, _b: B) -> A {
    a
}

pub struct Pair<T> {
    a: T,
    b: T,
}

struct Wrap<T>(T);

struct Two<K, V> {
    key: K,
    value: V,
}

impl<T> Pair<T> {
    fn new(a: T, b: T) -> Self {
        Pair { a, b }
    }

    fn into_a(self) -> T {
        self.a
    }

    fn swap(self) -> Pair<T> {
        Self {
            a: self.b,
            b: self.a,
        }
    }
}

// A block for one instance.
impl Pair<i32> {
    fn sum(&self) -> i32 {
        self.a + self.b
    }
}

impl<K, V> Two<K, V> {
    fn value(self) -> V {
        self.value
    }
}

// Parameters may shadow items of the same name.
struct T;

fn shadow<T>(x: T) -> T {
    x
}

// Generic arguments in types and expression paths, turbofish included.
fn args(p: Pair<i32>, w: Wrap<u8>) -> Two<i32, u8> {
    let n = id::<i32>(p.a);
    let q = Pair::<i32>::new(n, 2);
    let r: Pair<i32> = Pair::new(1, 2);
    let s = Pair::<u8> { a: 1, b: 2 };
    let _t: Pair<_> = Pair { a: true, b: false };
    let _u = first::<_, bool>(1u8, true);
    let _v = q.swap().into_a() + r.sum();
    let _w = Wrap::<u8>(s.a);
    Two {
        key: id(p.b),
        value: w.0,
    }
}

// Generic types nested in other types.
fn nested(x: Pair<Pair<i32>>, y: (Wrap<bool>, &Pair<u8>)) -> u8 {
    let _z: Wrap<Pair<i32>> = Wrap(x.a);
    y.1.a
}

fn main() {
    let _ = id(T);
    let _ = shadow(1);
}
