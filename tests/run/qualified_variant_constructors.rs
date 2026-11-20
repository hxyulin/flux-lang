enum Value<'a, T> {
    Unit,
    Text(&'a str, T),
}

type Alias<'a, T> = Value<'a, T>;

fn direct<'a>(text: &'a str) -> Value<'a, u8> {
    <Value<'a, u8>>::Text::<>(text, 3)
}

fn via_alias<'a>(text: &'a str) -> Value<'a, u8> {
    <Alias<'a, u8>>::Text(text, 4)
}

fn main() {
    let text = String::from("qualified");
    for value in [direct(&text), via_alias(&text), <Value<'static, u8>>::Unit] {
        match value {
            Value::Text(text, tag) => println!("{text} {tag}"),
            Value::Unit => println!("unit"),
        }
    }
}
