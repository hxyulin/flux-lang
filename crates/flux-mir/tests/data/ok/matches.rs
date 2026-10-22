// `match`, `if let` and `let`-`else` as MIR: each pattern tests the parts
// of the place it looks at, then binds; an enum is read through its
// discriminant and a downcast to the variant it holds.

struct Noisy;

impl Drop for Noisy {
    fn drop(&mut self) {}
}

enum E {
    A(i32),
    B { x: i32, y: bool },
    C,
}

fn arms(e: &E) -> i32 {
    match e {
        E::A(n) if *n > 0 => *n,
        E::A(_) | E::C => 0,
        E::B { x, y: true } => *x,
        E::B { .. } => -1,
    }
}

fn ranges(x: u8) -> u8 {
    match x {
        0 => 1,
        1..=9 => 2,
        _ => 3,
    }
}

fn condition(o: Option<Noisy>) -> bool {
    if let Some(n) = o {
        drop(n);
        true
    } else {
        false
    }
}

fn let_else(o: Option<i32>) -> i32 {
    let Some(v) = o else {
        return 0;
    };
    v
}

fn build() -> E {
    E::B { x: 1, y: false }
}

fn main() {}
