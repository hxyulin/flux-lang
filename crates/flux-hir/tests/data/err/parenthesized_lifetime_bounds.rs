// Lifetime bounds may not be parenthesized, as rustc reports.
fn f<'a, T: ('a)>(_: &'a T) {}

fn main() {}
