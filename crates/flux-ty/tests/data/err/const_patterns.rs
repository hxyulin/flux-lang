// Constants in patterns: errors found during type checking.

const B: bool = true;
const P: (i32, i32) = (1, 2);

trait Tr {
    const K: i32;
    fn f();
}

struct S;

impl Tr for S {
    const K: i32 = 9;
    fn f() {}
}

fn generic<T: Tr>(x: i32) -> i32 {
    match x {
        T::K => 1,
        _ => 0,
    }
}

fn method<T: Tr>(x: i32) -> i32 {
    match x {
        T::f => 1,
        _ => 0,
    }
}

fn bool_range(x: bool) -> i32 {
    match x {
        B..=B => 1,
        _ => 0,
    }
}

fn wrong_type(x: u8) -> i32 {
    match x {
        S::K => 1,
        _ => 0,
    }
}

fn tuple(x: (i32, i32)) -> i32 {
    match x {
        P => 1,
        _ => 0,
    }
}

fn main() {}
