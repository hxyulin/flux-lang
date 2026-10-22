// Literals in a discriminant's expression are checked against the
// enum's `repr` type (rustc's deny-by-default `overflowing_literals`).

#[repr(u8)]
enum OutOfRange {
    A = 255 + 0,
    B = (256),
}

#[repr(i8)]
enum Negative {
    A = { -129 },
    B = { -128 },
}

fn main() {}
