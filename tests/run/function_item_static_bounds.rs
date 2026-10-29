use core::any::Any;
struct Value(Box<dyn Any + Send + Sync>);
fn downcast<T: Any + Clone + Send + Sync + 'static>(value: &Value) -> &T {
    value.0.downcast_ref().unwrap()
}
fn values<'a, T: Any + Clone + Send + Sync + 'static>(
    input: &'a [Value],
) -> impl Iterator<Item = &'a T> {
    input.iter().map(downcast::<T> as fn(&Value) -> &T)
}
fn identity<'a, T: 'static>(value: &'a T) -> &'a T {
    value
}
trait Read<'a> {
    fn original(&self) -> &'a [u8];
}
struct Reader<'a>(&'a [u8]);
impl<'a> Read<'a> for Reader<'a> {
    fn original(&self) -> &'a [u8] {
        self.0
    }
}
fn convert<'de, 's, R: Read<'de>>(reader: &R, slice: &'s [u8]) -> &'s [u8] {
    assert_eq!(reader.original(), &[7, 8]);
    slice
}
fn parse<'de, 'r, R: Read<'de>, F>(reader: &'r R, callback: F) -> usize
where
    F: for<'s> FnOnce(&'r R, &'s [u8]) -> &'s [u8],
{
    let local = [1, 2, 3];
    let parsed = callback(reader, &local);
    assert_eq!(parsed.as_ptr(), local.as_ptr());
    parsed.len()
}
fn main() {
    let input = vec![
        Value(Box::new(String::from("one"))),
        Value(Box::new(String::from("two"))),
    ];
    let got: Vec<_> = values::<String>(&input).collect();
    assert_eq!(got[0], "one");
    assert_eq!(got[1], "two");
    assert_eq!(
        got[0] as *const String,
        downcast::<String>(&input[0]) as *const String
    );
    let callback: fn(&String) -> &String = identity::<String>;
    assert_eq!(callback(got[1]).as_ptr(), got[1].as_ptr());
    println!("{} {}", got[0], callback(got[1]));
    let data = [7, 8];
    println!("reader {}", parse(&Reader(&data), convert));
}
