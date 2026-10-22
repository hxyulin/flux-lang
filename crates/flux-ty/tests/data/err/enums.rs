// Type errors in enums, their constructors and patterns, and the
// expressions that match on them.

enum E {
    Unit,
    Tuple(i32),
    Record { x: i32 },
}

enum Maybe<T> {
    Just(T),
    Nothing,
}

fn constructors() {
    let _ = E::Tuple(1, 2);
    let _ = E::Unit();
    let _ = E::Record { x: 1, y: 2 };
    let _ = E::Record {};
    let _: Maybe<bool> = Maybe::Just(1);
    let _ = E::Tuple(true);
}

fn patterns(e: E, r: &mut E) {
    match e {
        E::Tuple(1, 2) => {}
        E::Record { y, .. } => {}
        E::Record {} => {}
        _ => {}
    }
    match e {
        1 => {}
        _ => {}
    }
    match r {
        &E::Tuple(x) => {}
        _ => {}
    }
    match true {
        false..=true => {}
    }
    match Maybe::Just(1) {
        Maybe::Just(true) => {}
        _ => {}
    }
}

fn arms(x: i32) -> i32 {
    let _ = match x {
        1 => 2u8,
        _ => 3i32,
    };
    let Maybe::Just(y) = Maybe::Just(x) else {
        5
    };
    y
}

fn casts(m: Maybe<i32>) {
    let _ = E::Unit as i32;
    let _ = m as i32;
}

fn names(e: E, x: u8, v: E::Unit) {
    let _ = E::Missing;
    e.nothing();
    match x {
        -1 => {}
        _ => {}
    }
    match &Some(1) {
        Some(&y) => {}
        None => {}
    }
}

fn casts_to_self() -> Result<i32, i32> {
    let _ = E::Unit as E;
    Err(4) as Result<i32, i32>
}

fn main() {}
