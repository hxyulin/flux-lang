enum Value<'a> {
    Text(&'a str),
}

trait Produce {
    type Output;
    fn output(&self) -> Self::Output;
}

struct Producer<'a>(&'a str);

impl<'a> Produce for Producer<'a> {
    type Output = Value<'a>;

    fn output(&self) -> Self::Output {
        Self::Output::Text(self.0)
    }
}

fn main() {
    let text = String::from("associated variant");
    let Value::Text(value) = Producer(&text).output();
    println!("{value}");
}
