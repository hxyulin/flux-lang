trait Mark { fn keep(self) -> Option<Self> where Self: Sized { Some(self) } }
#[derive(Clone, Copy)]
struct S<'a>(&'a i32);
impl Mark for S<'static> {}
fn main() { let n = 42; let _ = [S(&n)].into_iter().find_map(S::keep); }
