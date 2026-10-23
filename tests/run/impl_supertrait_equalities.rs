// Associated equalities on supertrait edges preserve the root argument's
// regions through both direct generic calls and iterator callbacks.
trait P {
    type A;
    fn get(self) -> Self::A;
}
trait C<T>: P<A = T> {}
struct S<'a>(&'a i32);
impl<'a> P for S<'a> {
    type A = &'a i32;
    fn get(self) -> &'a i32 {
        self.0
    }
}
impl<'a> C<&'a i32> for S<'a> {}
fn read<R, T: C<R>>(x: T) -> R {
    x.get()
}
fn read_option<R, T: C<R>>(x: T) -> Option<R> {
    Some(x.get())
}
fn direct(x: &i32) -> &i32 {
    read(S(x))
}
fn callback(x: &i32) -> &i32 {
    [S(x)].into_iter().find_map(read_option).unwrap()
}
fn main() {
    let x = 7;
    let y = 11;
    println!("{} {}", direct(&x), callback(&y));
}
