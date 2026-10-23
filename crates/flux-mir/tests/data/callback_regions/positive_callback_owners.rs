trait Mark {
    fn keep(self) -> Option<Self> where Self: Sized { Some(self) }
}
struct S<'a>(&'a i32);
impl Mark for S<'static> {}
struct Wrap<T>(T);
impl<T: Mark> Wrap<T> {
    fn keep(x: T) -> Option<T> { Some(x) }
}
fn main() {
    static N: i32 = 42;
    println!("{}", [S(&N)].into_iter().find_map(Wrap::keep).unwrap().0);
    println!("{}", [S(&N)].into_iter().find_map(S::keep).unwrap().0);
}
