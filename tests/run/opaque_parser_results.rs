use std::borrow::Cow;

trait Parser<I, O> {
    fn next(&mut self, input: &mut I) -> O;
}
impl<I, O, F: FnMut(&mut I) -> O> Parser<I, O> for F {
    fn next(&mut self, input: &mut I) -> O {
        self(input)
    }
}
fn trace<I, O, P: Parser<I, O>>(parser: P) -> impl Parser<I, O> {
    parser
}

fn borrowed<'a>(input: &mut &'a str) -> Cow<'a, str> {
    trace(|input: &mut &'a str| Cow::Borrowed(*input)).next(input)
}
fn part<'a>(input: &'a str, count: &mut u32) -> Option<Cow<'a, str>> {
    if *count < 2 {
        *count += 1;
        Some(Cow::Borrowed(input))
    } else {
        None
    }
}
fn basic_string<'a>(input: &mut &'a str) -> Result<Cow<'a, str>, ()> {
    trace(|input: &mut &'a str| {
        let mut count = 0;
        let mut value = Cow::Borrowed("");
        if let Some(first) = part(*input, &mut count) {
            value = first;
        }
        while let Some(next) = part(*input, &mut count) {
            value.to_mut().push_str(&next);
        }
        Ok(value)
    })
    .next(input)
}
fn make<'a>(text: &'a str) -> impl Fn() -> &'a str {
    move || text
}
fn invoke<'a, F: Fn() -> &'a str>(callback: F) -> &'a str {
    callback()
}

fn main() {
    let text = String::from("parser λ");
    let mut input = text.as_str();
    let value = borrowed(&mut input);
    assert!(matches!(value, Cow::Borrowed(_)));
    assert_eq!(value.as_ptr(), text.as_ptr());
    let joined = basic_string(&mut input).unwrap();
    assert!(matches!(joined, Cow::Owned(_)));
    assert_eq!(joined, "parser λparser λ");
    let output = invoke(make(&text));
    assert_eq!(output.as_ptr(), text.as_ptr());
    println!("{} | {} | {}", value, joined, output);
}
