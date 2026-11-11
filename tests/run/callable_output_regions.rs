use std::borrow::Cow;
type Encoder<'a> = Option<&'a dyn Fn(&str) -> Cow<'_, [u8]>>;
fn encode<'a>(callback: Encoder<'_>, input: &'a str) -> Cow<'a, [u8]> {
    if let Some(callback) = callback {
        return callback(input);
    }
    input.as_bytes().into()
}
fn bytes(input: &str) -> Cow<'_, [u8]> {
    Cow::Borrowed(input.as_bytes())
}
fn selected<'a>(input: &'a u8) -> Option<&'a u8> {
    Some(input)
}
fn select<'a>(input: &'a [u8]) -> Option<&'a u8> {
    Some(input).and_then(|input| {
        input
            .iter()
            .filter_map(selected as fn(&'a u8) -> Option<&'a u8>)
            .next()
    })
}
trait Value {
    fn number(&self) -> u32;
}
impl Value for u32 {
    fn number(&self) -> u32 {
        *self
    }
}
impl Value for &str {
    fn number(&self) -> u32 {
        self.len() as u32
    }
}
fn main() {
    let text = String::from("borrowed");
    let result = encode(Some(&bytes), &text);
    assert_eq!(&*result, b"borrowed");
    let data = [37u8, 41];
    assert_eq!(select(&data), Some(&37));
    let text = "hello";
    let number = 7u32;
    let fields = [(0u8, Some(&number as &dyn Value)), (1u8, Some(&text))];
    assert_eq!(fields[0].1.unwrap().number(), 7);
    assert_eq!(fields[1].1.unwrap().number(), 5);
    println!(
        "{} {} {}",
        result.len(),
        select(&data).unwrap(),
        fields[1].1.unwrap().number()
    );
}
