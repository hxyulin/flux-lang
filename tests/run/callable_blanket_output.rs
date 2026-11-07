trait Parse: Sized { fn parse(input: &u32) -> Result<Self, ()>; }
trait Parser { type Output; fn parse2(self, input: &u32) -> Result<Self::Output, ()>; }
impl<F, T> Parser for F where F: FnOnce(&u32) -> Result<T, ()> {
    type Output = T;
    fn parse2(self, input: &u32) -> Result<T, ()> { self(input) }
}
fn parse<T: Parse>(input: &u32) -> Result<T, ()> { T::parse.parse2(input) }
impl Parse for u32 { fn parse(input: &u32) -> Result<Self, ()> { Ok(*input) } }
fn main() { println!("{}", parse::<u32>(&42).unwrap()); }
