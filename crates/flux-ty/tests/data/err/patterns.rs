// Patterns checked once their types are known: `match`es must be
// exhaustive, `let` and parameter patterns irrefutable, and ranges
// non-empty, as rustc reports them.

enum E {
    A,
    B(i32, bool),
    C { x: i32, y: bool },
    D,
}

enum Void {}

struct S {
    a: bool,
    b: bool,
}

struct T(bool, u8);

fn variants(e: E) {
    match e {
        E::A => {}
    }
}

fn nested(o: Option<Option<bool>>) {
    match o {
        None => {}
        Some(None) => {}
    }
}

fn integers(x: i32, y: u8, z: i128, w: usize, v: isize) {
    match x {
        0 => {}
    }
    match y {
        0..=5 => {}
        7 => {}
    }
    match z {
        0 => {}
    }
    match w {
        0..=18446744073709551615 => {}
    }
    match v {
        0.. => {}
    }
}

fn many(x: u8) {
    match x {
        0 => {}
        2 => {}
        4 => {}
        6 => {}
        8 => {}
    }
}

fn structs(t: (bool, bool), s: S, u: T) {
    match t {
        (true, _) => {}
    }
    match s {
        S { a: true, .. } => {}
    }
    match u {
        T(true, _) => {}
    }
}

fn references(e: &E) {
    match e {
        E::A | E::B(..) => {}
    }
}

fn mutable(e: &mut E) {
    match e {
        E::A => {}
    }
}

fn guards(e: E, c: bool) {
    match e {
        E::A if c => {}
        E::B(..) | E::C { .. } | E::D => {}
    }
}

fn empty(x: i8, e: E, r: &E, b: bool) {
    match x {}
    match e {}
    match r {}
    match b {}
}

fn uninhabited(o: Option<Void>, v: Void) -> i32 {
    match o {
        None => {}
    }
    match v {}
}

fn refutable(e: E, x: u8) {
    let E::A = e;
    let 0 = x;
}

fn parameter(E::A: E) {}

fn ranges(x: u8) {
    match x {
        3..=1 => {}
        4..4 => {}
        _ => {}
    }
}

fn modes(o: &Option<i32>) {
    match o {
        Some(ref x) => {}
        Some(mut y) => {}
        None => {}
    }
}

fn main() {}
