// Guard captures must use the scrutinee until the arm takes ownership.
struct S(u8);

impl Drop for S {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

impl S {
    fn is(&self, value: u8) -> bool {
        self.0 == value
    }
}

struct Pair(S, u8);

fn shared(s: &S, value: u8) -> bool {
    s.is(value)
}

fn guards(value: u8) {
    match S(value) {
        b if (0..1).all(|_| b.0 == value) => println!("match {}", b.0),
        _ => println!("other"),
    }
    // A move closure can copy a field, even from a type with Drop.
    match S(value + 1) {
        b if (move || b.0 == value + 1)() => println!("copy {}", b.0),
        _ => println!("other"),
    }
    // Borrow the entire non-Copy value, including through nested closures.
    match S(value + 2) {
        b if (0..1).all(|_| (0..1).any(|_| shared(&b, value + 2))) => {
            println!("shared {}", b.0);
        }
        _ => println!("other"),
    }
    // Failed guards must leave ownership for later guards and arms.
    match Some(S(value + 3)) {
        Some(b) if (0..1).all(|_| b.is(0)) => println!("unexpected {}", b.0),
        Some(b) if (0..1).all(|_| b.is(value + 3)) => println!("later {}", b.0),
        _ => println!("other"),
    }
    match S(value + 4) {
        b if (0..1).all(|_| shared(&b, 0)) => println!("unexpected {}", b.0),
        b => println!("fallback {}", b.0),
    }
    match Pair(S(value + 5), value + 6) {
        b if {
            let copied = move || b.1;
            let borrowed = || b.0.is(value + 5);
            println!("pair {} {}", copied(), borrowed());
            false
        } => println!("unexpected {}", b.1),
        b => println!("pair fallback {} {}", b.0.0, b.1),
    }
    // Aliases also apply to guards nested within a closure body.
    let outer = || match S(value + 7) {
        b if (0..1).all(|_| b.0 == value + 7) => println!("outer {}", b.0),
        _ => println!("other"),
    };
    outer();
    // Copy bindings and reference bindings are initialized early, and can
    // still be captured by shared reference (or copied into move closures).
    match value {
        b if (move || b == value)() => println!("scalar {}", b),
        _ => println!("other"),
    }
    let mut s = S(value + 8);
    match s {
        ref mut b if (0..1).all(|_| shared(b, value + 8)) => println!("ref mut {}", b.0),
        _ => println!("other"),
    }
    match &s {
        b if (move || shared(b, value + 8))() => println!("ref {}", b.0),
        _ => println!("other"),
    }
}

fn main() {
    guards(7);
}
