trait P { type A; fn get(self) -> Self::A; }
trait C<T> { type N: P<A=T>; }
struct S<'a>(&'a i32);
impl<'a> P for S<'a> { type A = &'a i32; fn get(self)->Self::A {self.0} }
impl<'a> C<&'static i32> for S<'a> { type N = Self; }
fn read<R,T:C<R>>(x:(T,T::N))->Option<R> {Some(x.1.get())}
fn escape<'a>(x:&'a i32)->&'static i32 {[(S(x),S(x))].into_iter().find_map(read).unwrap()}
fn main() {}
