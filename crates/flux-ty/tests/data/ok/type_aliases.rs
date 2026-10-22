// Type aliases in signatures, bodies, `impl` blocks and paths.

#[derive(Clone, Copy, PartialEq)]
struct Pair<T> {
    a: T,
    b: T,
}

impl<T: Copy> Pair<T> {
    fn new(a: T, b: T) -> Self {
        Pair { a, b }
    }
    fn first(&self) -> T {
        self.a
    }
}

trait Total {
    fn total(&self) -> i64;
}

type Ints = Pair<i32>;
type Twice<T> = (T, T);
type Opt = Option<u8>;
type Id<T> = T;
type Nested<T> = Twice<Id<T>>;
type Ptr<T> = *const T;

impl Ints {
    fn sum(&self) -> i32 {
        self.a + self.b
    }
    const ZERO: Ints = Pair { a: 0, b: 0 };
}

impl Total for Ints {
    fn total(&self) -> i64 {
        self.sum() as i64
    }
}

mod shapes {
    pub enum Shape<T> {
        Circle(T),
        Square { side: T },
    }
    pub type Small = Shape<u8>;
}

use shapes::Small;

fn swap<T>(t: Nested<T>) -> Twice<T> {
    (t.1, t.0)
}

fn read(p: Ptr<u8>) -> u8 {
    unsafe { *p }
}

fn side(s: &Small) -> u8 {
    match s {
        Small::Circle(r) => *r,
        Small::Square { side } => *side,
    }
}

fn main() {
    let p = Ints::new(1, 2);
    let q: Id<Ints> = Ints { a: 3, b: 4 };
    let Ints { a, .. } = q;
    let z = Ints::ZERO;
    let o = Opt::Some(3);
    let n = match o {
        Opt::Some(x) => x,
        Opt::None => 0,
    };
    let t = swap((a, p.sum()));
    let c = Small::Square { side: n };
    let s = side(&c);
    let w = q.total() + z.first() as i64;
}
