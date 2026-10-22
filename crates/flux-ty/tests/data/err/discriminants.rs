// A discriminant written as an expression has the enum's `repr` type,
// `isize` without one, and is checked as a constant's value is.

const M: i32 = 1;

fn runtime() -> isize {
    1
}

enum NotAnInteger {
    A = true,
}

#[repr(u8)]
enum WrongInteger {
    A = M,
    B = 1 + 1u16,
}

enum Widened {
    A = M as isize,
    B = { 2i64 },
}

enum NotConst {
    A = runtime(),
}

fn main() {}
