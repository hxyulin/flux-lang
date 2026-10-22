// `Fn` bounds whose result's elided lifetime has no single one among
// their parameters to be (E0106).

fn none<F: Fn() -> &u8>(f: F) {}

fn two<F: Fn(&u8, &u8) -> &u8>(f: F) {}

fn main() {}
