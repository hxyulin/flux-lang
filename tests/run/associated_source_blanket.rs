use std::ops::Deref;

trait Source {
    type Slice<'a> where Self: 'a;
    fn slice(&self) -> Self::Slice<'_>;
}
impl Source for str {
    type Slice<'a> = &'a str;
    fn slice(&self) -> &str { self }
}
impl<T> Source for T where T: Deref, T::Target: Source {
    type Slice<'a> = <T::Target as Source>::Slice<'a> where T: 'a;
    fn slice(&self) -> Self::Slice<'_> { self.deref().slice() }
}
trait Token { type Source: Source + ?Sized; }
struct Text;
impl Token for Text { type Source = str; }
fn slice<T: Token>(s: &T::Source) -> <T::Source as Source>::Slice<'_> { s.slice() }
fn main() { println!("{}", slice::<Text>("borrowed")); }
