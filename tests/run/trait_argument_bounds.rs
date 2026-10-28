trait Variant<'de> { type Error; fn text(self) -> Result<&'de str, Self::Error>; }
trait Access<'de>: Sized {
    type Error;
    type Variant: Variant<'de, Error = Self::Error>;
    fn variant(self) -> Self::Variant;
    fn text(self) -> Result<&'de str, Self::Error> { self.variant().text() }
}
impl<'de> Variant<'de> for &'de str {
    type Error = ();
    fn text(self) -> Result<&'de str, ()> { Ok(self) }
}
struct Input<'a>(&'a str);
impl<'de> Access<'de> for Input<'de> {
    type Error = ();
    type Variant = &'de str;
    fn variant(self) -> Self::Variant { self.0 }
}
fn decode<'a, A: Access<'a>>(a: A) -> Result<&'a str, A::Error> { a.text() }
trait Failure {}
impl Failure for () {}
trait IntoReader<'de, E: Failure = ()> {
    type Reader: Variant<'de, Error = E>;
    fn into_reader(self) -> Self::Reader;
}
impl<'de> IntoReader<'de> for Input<'de> {
    type Reader = &'de str;
    fn into_reader(self) -> Self::Reader { self.0 }
}
fn through<'de, E: Failure, T: IntoReader<'de, E>>(input: T) -> Result<&'de str, E> {
    input.into_reader().text()
}
fn main() {
    let owned = String::from("borrowed variant");
    let output = decode(Input(&owned)).unwrap();
    assert_eq!(output.as_ptr(), owned.as_ptr());
    println!("{}", output);
    let generic = through::<(), _>(Input(&owned)).unwrap();
    assert_eq!(generic.as_ptr(), owned.as_ptr());
    println!("{}", generic);

}
