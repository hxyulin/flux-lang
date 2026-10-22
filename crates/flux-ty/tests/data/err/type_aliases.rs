struct Pair<T> {
    a: T,
    b: T,
}

impl<T> Pair<T> {
    fn new(a: T, b: T) -> Self {
        Pair { a, b }
    }
}

enum E {
    A,
}

type Ints = Pair<i32>;
type Twice<T> = (T, T);
type Unused<T> = i32;
type Missing = Nope;
type Placeholder = _;
type Cycle1 = Cycle2;
type Cycle2 = (u8, Cycle1);
type BadBound<T: Nope> = T;
type EA = E;

mod m {
    type Hidden = u8;
}

fn missing(t: Twice) {}
fn too_many(t: Twice<i32, u8>) {}
fn private(h: m::Hidden) {}

fn main() {}
