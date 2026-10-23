trait Mark {}
#[derive(Clone, Copy)]
struct S<'a>(&'a i32);
impl Mark for S<'static> {}
fn keep<T: Mark>(x: T) -> Option<T> { Some(x) }
fn main() { let n = 42; let _ = keep(S(&n)); }
