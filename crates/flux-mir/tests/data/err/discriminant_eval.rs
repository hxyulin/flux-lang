// Discriminants written as expressions are evaluated whether the enum
// is used or not, with rustc's errors: evaluation errors at the
// expression (E0080), a discriminant counted past the type's maximum at
// the variant (E0370), and a value assigned more than once at the enum
// (E0081).

const N: isize = 1;

enum DividedByZero {
    A = 1 / 0,
}

enum Overflowing {
    A = isize::MAX + 1,
}

#[repr(u8)]
enum OverflowingByte {
    A = 255 + 1,
}

const fn unwrapped() -> isize {
    let o: Option<isize> = None;
    o.unwrap()
}

enum Panicking {
    A = unwrapped(),
}

#[repr(u8)]
enum Counted {
    A = 254 + 1,
    B,
}

enum CountedTwice {
    A = isize::MAX - 1,
    B,
    C,
}

enum Duplicated {
    A = N,
    B = 2 - 1,
    C = 0,
    D,
}

#[repr(i8)]
enum DuplicatedNegative {
    A = -1,
    B = N as i8 - 2,
    C = 5,
    D = 2 + 3,
}

fn main() {}
