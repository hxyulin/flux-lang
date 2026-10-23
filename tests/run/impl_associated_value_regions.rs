trait R { type A; fn value(&self) -> Self::A; }
trait Q { type A; fn projected(&self) -> Self::A; }
struct S<'a>(&'a i32);
impl<'a> R for S<'a> {
    type A = i32;
    fn value(&self) -> i32 { *self.0 + 1 }
}
impl<'a> Q for S<'a> {
    type A = <Self as R>::A;
    fn projected(&self) -> Self::A { self.value() }
}
fn read<'a>(x: &'a i32) -> i32 { S(x).projected() }
fn main() { let x = 41; println!("{}", read(&x)); }
