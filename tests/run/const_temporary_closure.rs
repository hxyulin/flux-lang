//! A borrowed const temporary can store a capture-free closure's function pointer.

trait Test: Sync {
    fn accepts(&self, value: i32) -> bool;
}

struct Predicate {
    test: fn(i32) -> bool,
}

impl Predicate {
    const fn new(test: fn(i32) -> bool) -> Self {
        Self { test }
    }
}

impl Test for Predicate {
    fn accepts(&self, value: i32) -> bool {
        (self.test)(value)
    }
}

const TESTS: &[&dyn Test] = &[&Predicate::new(|value| value > 0)];

fn main() {
    println!("{} {}", TESTS[0].accepts(7), TESTS[0].accepts(-7));
}
