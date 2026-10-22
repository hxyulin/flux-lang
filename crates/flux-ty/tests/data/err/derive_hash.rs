// `#[derive(Hash)]` where a field's type does not implement `Hash`
// (E0277, at the field), on a struct and on an enum's variant, and a
// raw pointer to a temporary (E0745).

struct NoHash;

#[derive(Hash)]
struct S {
    a: u8,
    b: NoHash,
}

#[derive(Hash)]
enum E {
    A(u8),
    B { x: NoHash },
}

fn f() -> i32 {
    1
}

fn main() {
    let _p = &raw const f();
}
