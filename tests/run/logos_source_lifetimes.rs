use std::fmt::Debug;
use std::ops::Deref;
trait Source {
    type Slice<'a>: PartialEq + Eq + Debug where Self: 'a;
    fn slice(&self, start: usize, end: usize) -> Self::Slice<'_>;
}
impl Source for str {
    type Slice<'a> = &'a str;
    fn slice(&self, start: usize, end: usize) -> &str { &self[start..end] }
}
impl<T: Deref> Source for T where T::Target: Source {
    type Slice<'a> = <T::Target as Source>::Slice<'a> where Self: 'a;
    fn slice(&self, start: usize, end: usize) -> Self::Slice<'_> { self.deref().slice(start,end) }
}
trait Token<'s> { type Source: Source + ?Sized + 's; }
struct Text;
impl<'s> Token<'s> for Text { type Source = str; }
struct Lexer<'s,T:Token<'s>> { source: &'s T::Source }
impl<'s,T:Token<'s>> Lexer<'s,T> {
    fn slice(&self)-><T::Source as Source>::Slice<'s> { self.source.slice(0,3) }
}
enum CallbackResult<'a,T:Token<'a>> { Emit(T), Error(&'a T::Source) }
trait Callback<'a,P,T:Token<'a>> {
    fn construct<C>(self, callback:C)->CallbackResult<'a,T> where C:Fn(P)->T;
}
fn main() {
    let value=String::from("abc def");
    let lexer=Lexer::<Text>{source:value.as_str()};
    assert_eq!(lexer.slice(),"abc");
    assert_eq!(Source::slice(&value,4,7),"def");
    println!("{}",lexer.slice());
}
