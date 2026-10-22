// Arms that do not fit: as rustc's `CoerceMany`, the first is reported,
// at it, and the match's type is then an error, which the arms after it
// are not reported for.

fn expected_first(x: u8) -> i32 {
    match x {
        0 => "zero",
        1 => "one",
        _ => 3,
    }
}

fn expected_later(x: u8) -> i32 {
    match x {
        0 => 1,
        1 => "one",
        2 => true,
        _ => 3,
    }
}

fn annotated(x: u8) {
    let _v: i32 = match x {
        0 => "zero",
        1 => "one",
        _ => 3,
    };
}

fn unexpected(x: u8) -> i32 {
    let v = match x {
        0 => 1,
        1 => "one",
        2 => "two",
        _ => 3,
    };
    v
}

fn statement(x: u8) {
    match x {
        0 => 1,
        1 => "one",
        _ => 'c',
    };
}

// Each `if` is a match of its own.
fn chain(x: u8) -> i32 {
    if x == 0 { 1 } else if x == 1 { "s" } else { false }
}

fn main() {}
