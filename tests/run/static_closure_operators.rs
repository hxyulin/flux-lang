//! A closure initialized in const context runs overloaded operators at runtime.

#[derive(PartialEq)]
enum Mode {
    Scalar,
    Vector,
}

static EQUAL: fn(Mode, Mode) -> bool = |left, right| left == right;

const fn choose_different() -> fn(Mode, Mode) -> bool {
    |left, right| left != right
}

fn main() {
    let different = choose_different();
    println!(
        "{} {}",
        EQUAL(Mode::Scalar, Mode::Scalar),
        different(Mode::Scalar, Mode::Vector),
    );
}
