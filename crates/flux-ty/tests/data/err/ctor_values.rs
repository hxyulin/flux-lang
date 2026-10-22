// A constructor's value is a function item of its own type (E0308).

enum Atom {
    Class(u32),
}

struct W(u8);

fn main() {
    let a: i32 = Atom::Class;
    let b: i32 = W;
    let _ = (a, b);
}
