trait Signature {
    const VALUE: &'static str;
}

impl Signature for str {
    const VALUE: &'static str = "borrowed";
}

impl<T: Signature + ?Sized> Signature for &T {
    const VALUE: &'static str = T::VALUE;
}

struct Borrowed<'a>(&'a str);

impl<'a> Signature for Borrowed<'a> {
    const VALUE: &'static str = <&'a str as Signature>::VALUE;
}

struct Name<'a>(Borrowed<'a>);

impl<'a> Signature for Name<'a> {
    const VALUE: &'static str = <Borrowed<'a> as Signature>::VALUE;
}

struct Related<'a: 'b, 'b>(&'a str, &'b str);

impl<'a: 'b, 'b> Signature for Related<'a, 'b> {
    const VALUE: &'static str = "related";
}

fn related<'a: 'b, 'b>(_: &'a str, _: &'b str) -> &'static str {
    <Related<'a, 'b> as Signature>::VALUE
}

struct Equal<'a, 'b>(&'a str, &'b str);

impl<'a> Signature for Equal<'a, 'a> {
    const VALUE: &'static str = "equal";
}

fn equal<'a>(_: &'a str) -> &'static str {
    <Equal<'a, 'a> as Signature>::VALUE
}

fn name<'a>(value: Name<'a>) -> &'static str {
    assert_eq!(value.0.0, "local");
    <Name<'a> as Signature>::VALUE
}

fn main() {
    let first = String::from("local");
    let second = String::from("other");
    println!("{}", name(Name(Borrowed(&first))));
    println!("{}", related(&first, &second));
    println!("{}", equal(&second));
}
