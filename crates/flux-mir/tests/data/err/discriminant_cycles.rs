// Discriminants that need themselves (E0391), reported once, at the
// discriminant evaluation started from, as rustc does. A unit variant
// cast to an integer needs only that variant's discriminant, but a value
// of the enum (its layout) and a body matching on it need them all.

enum Itself {
    A = Itself::B as isize,
    B,
}

enum ThroughConst {
    A = FROM_ENUM + 1,
}

const FROM_ENUM: isize = ThroughConst::A as isize;

enum First {
    A = Second::X as isize,
    B,
}

enum Second {
    X = First::B as isize,
}

enum Matched {
    A = 1,
    B = matches(),
}

const fn matches() -> isize {
    match Matched::A {
        Matched::A => 5,
        _ => 6,
    }
}

#[derive(Clone, Copy)]
enum Valued {
    A = 1,
    B = value(Valued::A),
}

const fn value(v: Valued) -> isize {
    v as isize + 10
}

fn main() {}
