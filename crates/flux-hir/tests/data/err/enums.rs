// Errors found while lowering enums and patterns, as rustc reports them.

enum Dup {
    A,
    B,
    A,
}

enum Fields {
    X(pub i32),
    Y { pub y: i32 },
}

enum Discr {
    A = 1,
    B = 1,
}

enum Mixed {
    A(i32) = 1,
    B,
}

enum Overflow {
    A = 9223372036854775807,
    B,
}

enum E {
    Unit,
    Tuple(i32),
    Record { x: i32 },
}

impl E {
    fn self_ctor() {
        let _ = Self(1);
    }
}

fn values() {
    let _ = E::Record;
    let _ = E::Record(1);
    let _ = E;
    let _ = E { x: 1 };
}

fn patterns(e: E) {
    match e {
        E::Unit(..) => {}
        E::Tuple => {}
        E::Record => {}
        E::Record(..) => {}
        _ => {}
    }
    match (1, 2) {
        (x, y) | (y, 3) => {}
        (z, z) => {}
        _ => {}
    }
    match 1 {
        1...2 => {}
        _ => {}
    }
    let Tuple = e;
}

use E::Tuple;

fn conditions(x: Option<i32>) {
    let _ = let Some(y) = x;
}

enum Visible {
    pub A,
}

fn alternatives(Some(x) | None: Option<i32>) {}

struct S {
    x: i32,
    y: i32,
}

fn syntax(o: Option<i32>, a: bool, c: bool) {
    let Some(v) | None = o;
    let S { .., x } = S { x: 1, y: 2 };
    let Some(w) = if c { o } else { None } else {
        return;
    };
    let true = a && c else {
        return;
    };
    if let Some(z) = o || a {}
    let _ = Self::A;
}

fn variants(e: E) {
    match e {
        E::Missing(x) => {}
        E::Absent { .. } => {}
        E::Gone => {}
        _ => {}
    }
    match (1, 2) {
        (ref a, _) | (_, a) => {}
        (mut b, _) | (_, b) => {}
    }
    match Some(1) {
        Some(mut ref c) => {}
        None => {}
    }
}

fn main() {}
