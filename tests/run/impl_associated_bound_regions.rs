trait P {
    type A;
    fn get(self) -> Self::A;
}
trait C<T> {
    type N: P<A = T>;
}
struct S<'a>(&'a i32);
impl<'a> P for S<'a> {
    type A = &'a i32;
    fn get(self) -> Self::A {
        self.0
    }
}
impl<'a> C<&'a i32> for S<'a> {
    type N = Self;
}
fn read<R, T: C<R>>(_: T, x: T::N) -> R {
    x.get()
}
fn escape<'a>(x: &'a i32) -> &'a i32 {
    read(S(x), S(x))
}
fn main() {
    let x = 7;
    println!("{}", escape(&x));
}
