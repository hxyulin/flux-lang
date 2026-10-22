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
type EA = E;
type Bits = u8;

fn main() {
    let p = Ints::new(1u8, 2);
    let q = Ints { a: true, b: 1 };
    let t: Twice<u8> = (1, 2i32);
    let e = EA::B;
    let b: Bits = Bits::MAX + 1i32;
}
