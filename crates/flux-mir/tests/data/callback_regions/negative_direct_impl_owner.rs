trait Mark {}
struct S<'a>(&'a i32);
impl Mark for S<'static> {}
struct Wrap<T>(T);
impl<T: Mark> Wrap<T> {
    fn keep(x: T) -> Option<T> { Some(x) }
}
fn main() {
    let n = 42;
    let _ = Wrap::keep(S(&n));
}
