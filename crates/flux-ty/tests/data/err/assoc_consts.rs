// Associated constants that rustc rejects: in an `impl` block of a
// trait, a constant of another type (E0326), not in the trait (E0438) or
// that is a function there (E0323), or one left out (E0046); a path that
// finds several (E0034), none (E0599), or
// one whose bounds do not hold (E0277, E0599); and calling one. One of
// a trait for a type nothing says (`Tr::A`) is E0790, which rustc, like
// flux, leaves out once a body has other errors. A constant takes no
// generic arguments (E0107), and `Self::A` in a block must be its
// trait's (E0220).

trait Tr {
    const A: i32;
    const B: u8 = 1;
    fn f() {}
}

trait Other {
    const A: i32 = 5;
}

struct S;

impl Tr for S {
    const A: u8 = 1;
    const C: i32 = 3;
    const B: u8 = 2;
    const f: i32 = 0;
}

struct U;

impl Tr for U {}

impl Other for S {}

struct G<T>(T);

impl<T: Tr> G<T> {
    const E: i32 = T::A;
}

fn both<T: Tr + Other>() -> i32 {
    T::A
}

fn unbounded<T>() -> i32 {
    T::A
}

trait Typed {
    type A;
}

impl Typed for S {
    type A = u8;
}

impl Other for V {
    const A: Self::Nope = 3;
}

struct V;

fn main() {
    let generic = <S as Tr>::A::<u8>;
    let through = S::B::<u8, u16>;
    let x: i32 = Tr::A;
    let z = S::A;
    let w = <U as Other>::A;
    let g = G::<u8>::E;
    let c = <S as Tr>::B();
    let n: u8 = <S as Tr>::A;
}
