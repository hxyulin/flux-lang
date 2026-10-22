// Type aliases: in types, through modules and imports, and naming
// structs and enums in paths, struct expressions and patterns.

struct Pair<T> {
    a: T,
    b: T,
}

impl<T> Pair<T> {
    fn new(a: T, b: T) -> Self {
        Pair { a, b }
    }
}

enum Shape<T> {
    Circle(T),
    Square { side: T },
    Empty,
}

type Ints = Pair<i32>;
type Twice<T> = (T, T);
type Bounded<T: Copy> = Pair<T>;
type Bytes = Shape<u8>;

impl Ints {
    fn sum(&self) -> i32 {
        self.a + self.b
    }
}

mod m {
    pub type Count = u32;
    pub(crate) type Alias = super::Ints;
}

use m::Count;

fn swap<T>(t: Twice<T>) -> Twice<T> {
    (t.1, t.0)
}

fn shapes(s: Bytes) -> u8 {
    match s {
        Bytes::Circle(r) => r,
        Bytes::Square { side } => side,
        Bytes::Empty => 0,
    }
}

fn main() {
    let p = Ints::new(1, 2);
    let q = m::Alias { a: 3, b: 4 };
    let Ints { a, .. } = q;
    let c: Count = 5;
    let t = swap::<i32>((a, p.sum()));
    let s = Bytes::Circle(1);
    let e = Bytes::Empty;
    let b: Bounded<u8> = Pair::new(1, 2);
    let n = shapes(Bytes::Square { side: b.a });
}
