fn consume<'a, F: Fn(&'a str) -> &'a str>(s: &'a str, f: F) -> &'a str {
    f(s)
}
fn choose<'a, 'b, F: Fn(&'a str, &'b str) -> &'b str>(a: &'a str, b: &'b str, f: F) -> &'b str {
    f(a, b)
}
fn fixed<'a, F: FnOnce(&'a str) -> &'static str>(s: &'a str, f: F) -> &'static str {
    f(s)
}
fn make<'a>(s: &'a str) -> impl Fn() -> &'a str {
    move || s
}
fn combine<'a, P: Fn() -> &'a str, F: Fn(&'a str) -> &'a str>(p: P, f: F) -> &'a str {
    f(p())
}
fn identity_u32(value: &u32) -> &u32 {
    value
}
fn independent_output<'a>(_: &u32) -> &'a u32 {
    &7
}
fn invoke_pointer<F: for<'a> Fn(&'a u32) -> &'a u32>(f: F, value: &u32) -> u32 {
    *f(value)
}
fn forward_pointer<F: for<'a> Fn(&'a u32) -> &'a u32>(f: F, value: &u32) -> u32 {
    invoke_pointer(f, value)
}
fn typed<F: for<'r> Fn(&'r u32) -> &'r u32>(f: F) -> F {
    f
}
trait Read<A> {
    type Out;
    fn read(value: A) -> Self::Out;
}
struct Reader;
impl<'a> Read<&'a u32> for Reader {
    type Out = &'a u32;
    fn read(value: &'a u32) -> Self::Out {
        value
    }
}
fn read<'a, R: Read<&'a u32, Out = &'a u32>>(value: &'a u32) -> &'a u32 {
    R::read(value)
}
trait LifetimeRead<'a> {
    type Out;
    fn read(value: &'a u32) -> Self::Out;
}
impl<'a> LifetimeRead<'a> for Reader {
    type Out = &'a u32;
    fn read(value: &'a u32) -> Self::Out {
        value
    }
}
fn lifetime_read<'a, R: LifetimeRead<'a, Out = &'a u32>>(value: &'a u32) -> &'a u32 {
    R::read(value)
}
struct Guard<T, F: FnMut(&mut T)> {
    value: T,
    f: F,
}
impl<T, F: FnMut(&mut T)> Guard<T, F> {
    fn new(value: T, f: F) -> Self {
        Self { value, f }
    }
}
impl<T, F: FnMut(&mut T)> Drop for Guard<T, F> {
    fn drop(&mut self) {
        (self.f)(&mut self.value)
    }
}
fn chained_sum<'a>(items: &'a [u32], more: impl Iterator<Item = &'a u32>) -> u32 {
    let mut sum = 0;
    for value in items.iter().map(|v| v).chain(more) {
        sum += *value;
    }
    sum
}
struct ParserInput<'a>(&'a str);
trait Parser<I, O> {
    fn next(&mut self, input: &mut I) -> O;
}
impl<I, O, F: FnMut(&mut I) -> O> Parser<I, O> for F {
    fn next(&mut self, input: &mut I) -> O {
        self(input)
    }
}
fn cut<I, O, P: Parser<I, O>>(parser: P) -> impl Parser<I, O> {
    parser
}
fn both<I, O, P: Parser<I, O>, Q: Parser<I, O>>(first: P, second: Q) -> (P, Q) {
    (first, second)
}
fn parse_len(input: &mut ParserInput<'_>) -> usize {
    input.0.len()
}
fn parse_text<'a>(input: &mut ParserInput<'a>) -> &'a str {
    input.0
}
fn parse_nested<'a, 'b>(
    first: &mut ParserInput<'a>,
    second: &mut ParserInput<'b>,
) -> (&'a str, &'b str) {
    let (mut p, mut q) = both(parse_text, parse_text);
    assert_eq!(p.next(first).as_ptr(), first.0.as_ptr());
    assert_eq!(q.next(first).as_ptr(), first.0.as_ptr());
    assert_eq!(cut(parse_len).next(first), first.0.len());
    assert_eq!(cut(parse_len).next(second), second.0.len());
    (cut(parse_text).next(first), cut(parse_text).next(second))
}
struct DocumentLocated<I>(I, I);
struct DocumentStateful<I, S>(I, S);
type DocumentInput<'a> = DocumentStateful<DocumentLocated<&'a str>, ()>;
fn document_value(input: &mut DocumentInput<'_>) -> Result<u32, ()> {
    Ok(input.0.1.len() as u32)
}
fn document_parser<'state, 'input>(
    state: &'state std::cell::RefCell<u32>,
) -> impl Parser<DocumentInput<'input>, Result<u32, ()>> + 'state {
    move |input: &mut DocumentInput<'input>| {
        *state.borrow_mut() += 1;
        cut(document_value).next(input)
    }
}
fn main() {
    let state = std::cell::RefCell::new(0);
    let mut document = document_parser(&state);
    {
        let text = String::from("temporary");
        let mut input = DocumentStateful(DocumentLocated(text.as_str(), text.as_str()), ());
        assert_eq!(document.next(&mut input).unwrap(), 9);
    }
    drop(document);
    assert_eq!(*state.borrow(), 1);
    let number = 43;
    let mut drops = 0;
    {
        let _guard = Guard::new(&number, |value: &mut &u32| drops += **value);
    }
    assert_eq!(drops, 43);
    assert_eq!(chained_sum(&[2, 3], [5, 7].iter()), 17);
    assert!(core::ptr::eq(read::<Reader>(&number), &number));
    assert!(core::ptr::eq(lifetime_read::<Reader>(&number), &number));
    assert_eq!(forward_pointer(identity_u32, &number), 43);
    let pointer: for<'a> fn(&'a u32) -> &'a u32 = independent_output;
    assert_eq!(forward_pointer(pointer, &number), 7);
    assert_eq!(forward_pointer(&pointer, &number), 7);
    let identity = typed(|value| value);
    let literal = typed(|_| &7);
    assert_eq!(forward_pointer(&identity, &number), 43);
    assert_eq!(forward_pointer(&literal, &number), 7);
    let first = String::from("first");
    let second = String::from("second");
    let (left, right) = parse_nested(&mut ParserInput(&first), &mut ParserInput(&second));
    assert_eq!(left.as_ptr(), first.as_ptr());
    assert_eq!(right.as_ptr(), second.as_ptr());
    let result = consume(first.as_str(), |v| v);
    assert_eq!(result.as_ptr(), first.as_ptr());
    let result = choose(first.as_str(), second.as_str(), |_, b| b);
    assert_eq!(result.as_ptr(), second.as_ptr());
    assert_eq!(combine(make(first.as_str()), |v| v), "first");
    assert_eq!(fixed(second.as_str(), |_| "constant"), "constant");
    println!(
        "closure outputs {} {}",
        consume(first.as_str(), |v| v),
        result
    );
}
