trait Read<'a, T> {
    type Out;
    fn read(self, value: &'a T) -> Self::Out;
}
struct Reader;
impl<'a, T: 'a> Read<'a, T> for Reader {
    type Out = &'a T;
    fn read(self, value: &'a T) -> Self::Out {
        value
    }
}
fn reader<'a, T: 'a>() -> impl Read<'a, T, Out = &'a T> {
    Reader
}
fn forward<'a, T: 'a, R: Read<'a, T, Out = &'a T>>(r: R, value: &'a T) -> &'a T {
    r.read(value)
}
trait Pass<T> {
    fn pass(self, value: T) -> T;
}
impl<T> Pass<T> for Reader {
    fn pass(self, value: T) -> T {
        value
    }
}
fn passer<'a>() -> impl Pass<&'a str> {
    Reader
}
fn main() {
    ranked_opaque_cases();
    abstract_parser_cases();
    projected_parser_cases();
    let owned = String::from("opaque borrowed data");
    let count = 73u32;
    let result = forward(reader(), &count);
    let text = passer().pass(&owned);
    assert_eq!(*result, 73);
    assert_eq!(text.as_ptr(), owned.as_ptr());
    println!("{} {}", result, text);
}

trait Stream {
    type Token;
    fn take(&mut self) -> Self::Token;
}
struct Input<'a>(&'a u32);
impl<'a> Stream for Input<'a> {
    type Token = &'a u32;
    fn take(&mut self) -> Self::Token {
        self.0
    }
}
struct PairInput<'a, 'b>(&'a u32, &'b u32);
impl<'a, 'b> Stream for PairInput<'a, 'b> {
    type Token = (&'b u32, &'a u32);
    fn take(&mut self) -> Self::Token {
        (self.1, self.0)
    }
}
trait Parser<I, O> {
    fn parse(&mut self, input: &mut I) -> O;
}
struct Run;
impl<I: Stream> Parser<I, I::Token> for Run {
    fn parse(&mut self, input: &mut I) -> I::Token {
        input.take()
    }
}
fn parser<I: Stream>() -> impl Parser<I, I::Token> {
    Run
}
fn identity<T>(x: T) -> T {
    x
}
fn apply<T, F: Fn(T) -> T>(f: F, x: T) -> T {
    f(x)
}
struct StaticInput;
impl Stream for StaticInput {
    type Token = &'static u32;
    fn take(&mut self) -> Self::Token {
        &STATIC_COUNT
    }
}
static STATIC_COUNT: u32 = 149;
fn projected_parser_cases() {
    let left = 107;
    let mut tagged = MarkedInput::<u8>(&left, core::marker::PhantomData);
    assert_eq!(marked(&mut tagged) as *const u32, &left as *const u32);
    let right = 131;
    let mut input = Input(&left);
    let value = parser().parse(&mut input);
    assert_eq!(*value, 107);
    assert_eq!(value as *const u32, &left as *const u32);
    let mut pair = PairInput(&left, &right);
    let (b, a) = parser().parse(&mut pair);
    assert_eq!((*a, *b), (107, 131));
    assert_eq!(a as *const u32, &left as *const u32);
    assert_eq!(b as *const u32, &right as *const u32);
    let id = identity::<<Input<'_> as Stream>::Token>(&left);
    assert_eq!(id as *const u32, &left as *const u32);
    let borrowed_function = identity::<<Input<'_> as Stream>::Token>;
    let borrowed_callback = apply(borrowed_function, &left);
    assert_eq!(borrowed_callback as *const u32, &left as *const u32);
    let function = identity::<<StaticInput as Stream>::Token>;
    let callback = apply(function, &STATIC_COUNT);
    assert_eq!(callback as *const u32, &STATIC_COUNT as *const u32);
    println!("projected opaque {} {} {}", a, b, callback);
}

trait Marker {}
impl Marker for u8 {}
struct MarkedInput<'a, T>(&'a u32, core::marker::PhantomData<T>);
impl<'a, T: Marker> Stream for MarkedInput<'a, T> {
    type Token = &'a u32;
    fn take(&mut self) -> Self::Token {
        self.0
    }
}
fn marked<'a, T: Marker>(input: &mut MarkedInput<'a, T>) -> &'a u32 {
    parser().parse(input)
}

trait CompareLiteral<L> {}
impl<'a, L> CompareLiteral<L> for Input<'a> {}
struct LiteralParser<L>(L);
impl<L, I: Stream + CompareLiteral<L>> Parser<I, I::Token> for LiteralParser<L> {
    fn parse(&mut self, input: &mut I) -> I::Token {
        input.take()
    }
}
fn literal_parser<L, I: Stream + CompareLiteral<L>>(literal: L) -> impl Parser<I, I::Token> {
    LiteralParser(literal)
}
fn trace_parser<I, O, P: Parser<I, O>>(parser: P) -> impl Parser<I, O> {
    parser
}
fn abstract_run<'s, I: Stream + CompareLiteral<&'s str>>(
    literal: &'s str,
    input: &mut I,
) -> I::Token {
    trace_parser(literal_parser(literal)).parse(input)
}
impl<'s, I: Stream + CompareLiteral<&'s str>> Parser<I, I::Token> for &'s str {
    fn parse(&mut self, input: &mut I) -> I::Token {
        literal_parser(*self).parse(input)
    }
}
fn abstract_parser_cases() {
    let value = 173;
    let mut input = Input(&value);
    let result = {
        let literal = String::from("short-lived literal");
        abstract_run(literal.as_str(), &mut input)
    };
    assert_eq!(result as *const u32, &value as *const u32);
    let mut literal = "direct impl";
    let direct = Parser::parse(&mut literal, &mut input);
    assert_eq!(direct as *const u32, &value as *const u32);
    println!("abstract parser {} {}", result, direct);
}

fn ranked_opaque_identity() -> impl for<'a> Fn(&'a u32) -> &'a u32 {
    |value| value
}
fn invoke_ranked<'a, F: for<'r> Fn(&'r u32) -> &'r u32>(callback: F, value: &'a u32) -> &'a u32 {
    callback(value)
}
fn ranked_opaque_counter() -> impl FnMut(&u32) -> u32 {
    let mut total = 0;
    move |value: &u32| {
        total += *value;
        total
    }
}
fn invoke_counter<F: FnMut(&u32) -> u32>(mut callback: F) -> u32 {
    callback(&2) + callback(&3)
}
fn ranked_opaque_sum() -> impl Fn(&u32, &u32) -> u32 {
    |a, b| *a + *b
}
fn invoke_independent<F: for<'a, 'b> Fn(&'a u32, &'b u32) -> u32>(callback: F) -> u32 {
    callback(&5, &7)
}
fn ranked_opaque_cases() {
    let value = 211;
    let result = invoke_ranked(ranked_opaque_identity(), &value);
    assert_eq!(result as *const u32, &value as *const u32);
    assert_eq!(invoke_counter(ranked_opaque_counter()), 7);
    assert_eq!(invoke_independent(ranked_opaque_sum()), 12);
}
