trait Stream {
    type Token;
    fn take(&mut self) -> Self::Token;
}
struct Input(u8);
impl Stream for Input {
    type Token = u8;
    fn take(&mut self) -> u8 {
        let t = self.0;
        self.0 += 1;
        t
    }
}
struct Locating<I>(I);
impl<I: Stream> Stream for Locating<I> {
    type Token = I::Token;
    fn take(&mut self) -> I::Token {
        self.0.take()
    }
}
struct Stateful<I, S>(I, S);
impl<I: Stream, S: core::fmt::Debug> Stream for Stateful<I, S> {
    type Token = I::Token;
    fn take(&mut self) -> I::Token {
        self.0.take()
    }
}
type Data = Stateful<Locating<Input>, ()>;
trait Contains<T> {
    fn contains(&self, t: &T) -> bool;
}
impl<T: PartialEq> Contains<T> for (T, T) {
    fn contains(&self, t: &T) -> bool {
        self.0.eq(t) || self.1.eq(t)
    }
}
trait Parser<I, O> {
    fn parse(&mut self, input: &mut I) -> O;
    fn then_token(self) -> ThenToken<Self, I, O>
    where
        Self: Sized,
        I: Stream,
    {
        ThenToken(self, core::marker::PhantomData)
    }
    fn try_map<F, O2, E2>(self, map: F) -> TryMapped<Self, F, I, O, O2, E2>
    where
        Self: Sized,
        F: FnMut(O) -> Result<O2, E2>,
    {
        TryMapped(self, map, core::marker::PhantomData)
    }
    fn map<F, O2>(self, map: F) -> Mapped<Self, F, I, O, O2>
    where
        Self: Sized,
        F: FnMut(O) -> O2,
    {
        Mapped {
            parser: self,
            map,
            marker: core::marker::PhantomData,
        }
    }
}
struct ThenToken<P, I, O>(P, core::marker::PhantomData<(I, O)>);
impl<P, I, O> Parser<I, I::Token> for ThenToken<P, I, O>
where
    I: Stream,
    P: Parser<I, O>,
{
    fn parse(&mut self, input: &mut I) -> I::Token {
        let _ = self.0.parse(input);
        input.take()
    }
}
struct TryMapped<P, F, I, O, O2, E2>(P, F, core::marker::PhantomData<(I, O, O2, E2)>);
impl<P, F, I, O, O2, E2> Parser<I, Result<O2, E2>> for TryMapped<P, F, I, O, O2, E2>
where
    P: Parser<I, O>,
    F: FnMut(O) -> Result<O2, E2>,
{
    fn parse(&mut self, input: &mut I) -> Result<O2, E2> {
        (self.1)(self.0.parse(input))
    }
}
struct Mapped<P, F, I, O, O2> {
    parser: P,
    map: F,
    marker: core::marker::PhantomData<(I, O, O2)>,
}
impl<P, F, I, O, O2> Parser<I, O2> for Mapped<P, F, I, O, O2>
where
    P: Parser<I, O>,
    F: FnMut(O) -> O2,
{
    fn parse(&mut self, input: &mut I) -> O2 {
        (self.map)(self.parser.parse(input))
    }
}
struct One<I, S>(S, core::marker::PhantomData<I>);
impl<I: Stream, S: Contains<I::Token>> Parser<I, I::Token> for One<I, S> {
    fn parse(&mut self, input: &mut I) -> I::Token {
        let t = input.take();
        assert!(self.0.contains(&t));
        t
    }
}
fn one<I: Stream, S: Contains<I::Token>>(set: S) -> impl Parser<I, I::Token> {
    One(set, core::marker::PhantomData)
}
struct Node<T>(T);
struct Wrapped;
impl<I: Stream> Parser<I, Node<I::Token>> for Wrapped {
    fn parse(&mut self, input: &mut I) -> Node<I::Token> {
        Node(input.take())
    }
}
fn wrapped<I: Stream>() -> impl Parser<I, Node<I::Token>> {
    Wrapped
}
fn token(input: &mut Data) -> u8 {
    one((7u8, 8u8)).parse(input)
}
fn node(input: &mut Data) -> Node<u8> {
    wrapped().parse(input)
}
fn mapped(input: &mut Data) -> u8 {
    one((10u8, 11u8))
        .map(|value| match value {
            10u8 => 13u8,
            _ => 17u8,
        })
        .parse(input)
}
fn force<I, P: Parser<I, u8>>(parser: P) -> P {
    parser
}
fn forced(input: &mut Data) -> u8 {
    force(one((12u8, 13u8))).parse(input)
}
fn scale(value: u8) -> Result<u16, u32> {
    if value < 16 {
        Ok(u16::from(value) * 10)
    } else {
        Err(u32::from(value))
    }
}
fn try_next(input: &mut Data) -> Result<u16, u32> {
    one((13u8, 15u8)).then_token().try_map(scale).parse(input)
}
fn main() {
    let mut input = Stateful(Locating(Input(7)), ());
    assert_eq!(token(&mut input), 7);
    assert_eq!(token(&mut input), 8);
    let Node(t) = node(&mut input);
    assert_eq!(t, 9);
    assert_eq!(input.0.0.0, 10);
    assert_eq!(mapped(&mut input), 13);
    assert_eq!(mapped(&mut input), 17);
    assert_eq!(input.0.0.0, 12);
    assert_eq!(forced(&mut input), 12);
    assert_eq!(input.0.0.0, 13);
    assert_eq!(try_next(&mut input).unwrap(), 140);
    assert_eq!(try_next(&mut input).unwrap_err(), 16);
    assert_eq!(input.0.0.0, 17);
    println!("projected tokens {} {}", t, input.0.0.0);
}
