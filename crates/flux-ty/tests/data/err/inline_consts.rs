// Inline constants (`const { .. }`) are checked as constants are: they
// may call only `const fn`s (E0015).

fn f() -> i32 {
    3
}

const fn g() -> i32 {
    4
}

fn main() {
    let a = const { f() };
    let b = const { g() + 1 };
    let _ = (a, b);
}
