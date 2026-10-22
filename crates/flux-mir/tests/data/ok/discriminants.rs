// Discriminants written as expressions are evaluated with the other
// constants. A unit variant cast to an integer is its discriminant's
// constant (plus the variants since the last written one); a match
// tests the evaluated values.

const N: isize = 10;

const fn twice(x: isize) -> isize {
    x * 2
}

#[derive(Clone, Copy)]
enum E {
    A = 1 << 2,
    B = N + 1,
    C,
    D = twice(3),
}

#[repr(u8)]
enum R {
    X = 200 + 50,
    Y,
}

fn cast() -> i64 {
    E::A as i64 + E::C as i64 + R::Y as i64
}

fn pick(e: E) -> i32 {
    match e {
        E::A => 1,
        E::B | E::C => 2,
        E::D => 3,
    }
}

fn value(e: E) -> isize {
    e as isize
}

fn main() {
    let _ = cast() + pick(E::C) as i64 + value(E::D) as i64;
}
