trait Null {
    type Value;
    fn value() -> Self::Value;
}

struct Borrowed<'a>(&'a str);

impl<'a> Null for Borrowed<'a> {
    type Value = &'a str;

    fn value() -> Self::Value {
        "null"
    }
}

struct Owned(Borrowed<'static>);

impl Null for Owned {
    type Value = <Borrowed<'static> as Null>::Value;

    fn value() -> Self::Value {
        Borrowed::value()
    }
}

fn main() {
    println!("{}", Owned::value());
}
