//! Type aliases: generic and not, in modules and imported, in
//! signatures and bodies, as `impl` blocks' types, and naming structs and
//! enums in paths (`Alias::new()`), struct expressions and patterns.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

#[derive(Clone, Copy)]
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

type IntPair = Pair<i32>;
type Tuple<T> = (T, T);
type Opt = Option<u8>;
type Id<T> = T;
type Num = u64;
type Swapped<T> = Tuple<Id<T>>;

impl IntPair {
    fn sum(&self) -> i32 {
        self.a + self.b
    }
    const ORIGIN: IntPair = Pair { a: 0, b: 0 };
}

mod m {
    pub type Count = u32;
    pub enum Shape {
        Circle(u32),
        Square { side: u32 },
    }
    pub type S = Shape;
}

use m::{Count, S};

fn swap<T: Copy>(t: Swapped<T>) -> Tuple<T> {
    (t.1, t.0)
}

fn area(s: &S) -> Count {
    match s {
        S::Circle(r) => 3 * r * r,
        S::Square { side } => side * side,
    }
}

fn pick(o: Opt) -> u8 {
    match o {
        Opt::Some(x) => x,
        Opt::None => 0,
    }
}

fn main() {
    let p = IntPair::new(1, 2);
    let q = IntPair { a: 3, b: 4 };
    let IntPair { a, .. } = q;
    let x = pick(Opt::Some(3)) + pick(Opt::None);
    let t: Tuple<i32> = swap((5, 6));
    let n: Id<Num> = 7;
    let c = S::Circle(2);
    let sq = S::Square { side: 3 };
    let o = IntPair::ORIGIN;
    unsafe {
        printf(
            c"%d %d %d %d %d %d %d %llu %u %u %d\n".as_ptr(),
            p.sum(),
            q.first(),
            a,
            x as i32,
            t.0,
            t.1,
            p.b,
            n,
            area(&c),
            area(&sq),
            o.sum(),
        );
    }
}
