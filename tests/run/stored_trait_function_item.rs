enum Parsed<'a> {
    Text(&'a str),
}

trait Parse<'a>: Sized {
    fn parse(text: &'a str, nested: bool) -> Option<Self>;
}

impl<'a> Parse<'a> for Parsed<'a> {
    fn parse(text: &'a str, nested: bool) -> Option<Self> {
        if nested {
            with_parser(text, Self::parse)
        } else {
            Some(Self::Text(text))
        }
    }
}

fn with_parser<'a, T: 'a>(text: &'a str, parse: impl FnOnce(&'a str, bool) -> Option<T>) -> Option<T> {
    assert_eq!(std::mem::size_of_val(&parse), 0);
    parse(text, false)
}

fn parse_twice<'a>(text: &'a str) {
    let item = <Parsed<'a> as Parse<'a>>::parse;
    let Parsed::Text(direct) = item(text, true).unwrap();
    let pointer: fn(&'a str, bool) -> Option<Parsed<'a>> = item;
    let Parsed::Text(indirect) = pointer(text, false).unwrap();
    println!("{direct} {indirect}");
}

fn main() {
    let text = String::from("callback");
    parse_twice(&text);
}
