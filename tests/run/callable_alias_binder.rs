struct Buffer<'a> { text: &'a str }
type Stream<'a> = &'a Buffer<'a>;
trait Parse: Sized { fn parse(input: Stream) -> Result<Self, ()>; }
trait Parser { type Output; fn parse2(self, input: Stream) -> Result<Self::Output, ()>; }
impl<F, T> Parser for F where F: FnOnce(Stream) -> Result<T, ()> {
    type Output = T;
    fn parse2(self, input: Stream) -> Result<T, ()> { self(input) }
}
fn parse<T: Parse>(input: Stream) -> Result<T, ()> { T::parse.parse2(input) }
impl Parse for usize { fn parse(input: Stream) -> Result<Self, ()> { Ok(input.text.len()) } }
fn main() { let buffer = Buffer { text: "hello" }; println!("{}", parse::<usize>(&buffer).unwrap()); }
