trait Source<'a> {
    type Src: ?Sized;
}
trait Dest<'a> {
    type Dst: ?Sized;
    fn f(self) -> Result<&'a Self::Dst, (&'a Self::Src,)>
    where
        Self: Source<'a>,
        Self::Src: std::fmt::Debug,
        Self::Dst: std::fmt::Debug;
}
struct Pair<'a>(&'a str);
impl<'a> Source<'a> for Pair<'a> {
    type Src = str;
}
impl<'a> Dest<'a> for Pair<'a> {
    type Dst = str;
    fn f(self) -> Result<&'a str, (&'a <Self as Source<'a>>::Src,)> {
        Ok(self.0)
    }
}
fn call<'a, T: Source<'a, Src = str> + Dest<'a, Dst = str>>(x: T) -> &'a str {
    x.f().unwrap()
}
fn main() {
    let s = String::from("captured");
    assert_eq!(call(Pair(&s)), "captured");
    println!("projected lifetime method bounds passed");
}
