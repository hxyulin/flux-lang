// `#[derive(Default)]` on enums needs exactly one `#[default]` variant,
// without fields; `#[default]` is unknown without the derive, and takes
// no value.

#[derive(Default)]
enum NoDefault {
    A,
    B,
}

#[derive(Default)]
enum Two {
    #[default]
    A,
    #[default]
    B,
}

#[derive(Default)]
enum WithFields {
    #[default]
    A(i32),
    B,
}

enum NotDerived {
    #[default]
    A,
}

#[derive(Default)]
enum Valued {
    #[default = 1]
    A,
}

#[derive(Default)]
enum Fine {
    A,
    #[default]
    B,
}

fn main() {
    let _ = Fine::default();
    let _ = Valued::default();
}
