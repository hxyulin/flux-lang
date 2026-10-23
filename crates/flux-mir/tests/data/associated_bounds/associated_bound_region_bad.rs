trait P { type A; }
trait C<T> { type N: P<A=T>; }
struct S<'a>(&'a i32);
impl<'a> P for S<'a> { type A = &'static i32; }
impl<'a> C<&'a i32> for S<'a> { type N = Self; }
fn main() {}
