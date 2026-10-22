// Discriminants written as expressions other than literals: each is an
// anonymous constant of the enum's `repr` type, in the enum's module,
// where `Self` is the enum. Variants without one count from the last.

const N: isize = 10;

const fn twice(x: isize) -> isize {
    x * 2
}

enum E {
    A = 1 << 2,
    B = N + 1,
    C,
    D = twice(3),
    F = 50,
    G,
}

#[repr(u8)]
enum R {
    X = { let x = 200; x + 50 },
    Y,
    Z = R::X as u8 - 5,
}

enum S {
    A = 3,
    B = Self::A as isize + 1,
}

#[repr(u16)]
enum Message<T> {
    Quit = 1 + 1,
    Move(T) = 600,
    Write(T),
}

mod m {
    const HIDDEN: i32 = 7;

    pub enum Inner {
        A = HIDDEN as isize,
    }
}

fn main() {}
