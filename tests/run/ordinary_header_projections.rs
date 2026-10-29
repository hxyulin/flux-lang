trait Stream {
    type Slice;
    fn take(&mut self) -> Self::Slice;
}
trait Compare<L> {}
trait Parser<I, O, E> {
    fn parse_next(&mut self, input: &mut I) -> Result<O, E>;
}
impl<'s, I: Stream + Compare<&'s str>, E> Parser<I, I::Slice, E> for &'s str {
    fn parse_next(&mut self, input: &mut I) -> Result<I::Slice, E> {
        literal_parser(*self).parse_next(input)
    }
}
struct Literal<L>(L);
impl<L, I: Stream + Compare<L>, E> Parser<I, I::Slice, E> for Literal<L> {
    fn parse_next(&mut self, input: &mut I) -> Result<I::Slice, E> {
        Ok(input.take())
    }
}
fn literal_parser<L, I: Stream + Compare<L>, E>(value: L) -> impl Parser<I, I::Slice, E> {
    Literal(value)
}
struct Input<'a>(&'a str);
impl<'a> Stream for Input<'a> {
    type Slice = &'a str;
    fn take(&mut self) -> Self::Slice {
        self.0
    }
}
impl<'a, 's> Compare<&'s str> for Input<'a> {}
struct MutInput<'a>(Option<&'a mut u32>);
impl<'a> Stream for MutInput<'a> {
    type Slice = &'a mut u32;
    fn take(&mut self) -> Self::Slice {
        self.0.take().unwrap()
    }
}
impl<'a, 's> Compare<&'s str> for MutInput<'a> {}
fn main() {
    let data = String::from("original input allocation");
    let mut input = Input(data.as_str());
    let result = {
        let text = String::from("short-lived parser");
        let mut parser = text.as_str();
        let parsed: Result<&str, ()> = Parser::parse_next(&mut parser, &mut input);
        parsed.unwrap()
    };
    assert_eq!(result, data.as_str());
    assert_eq!(result.as_ptr(), data.as_ptr());
    let mut value = 17;
    let mut mutable = MutInput(Some(&mut value));
    let output = {
        let text = String::from("other parser");
        let mut parser = text.as_str();
        let parsed: Result<&mut u32, ()> = Parser::parse_next(&mut parser, &mut mutable);
        parsed.unwrap()
    };
    *output += 8;
    assert_eq!(*output, 25);
    println!("ordinary header {} {}", result, *output);
}
