trait Parser<I> { type Error; }
trait Error<I> { fn from_input(input: I) -> Self; }
struct Parse;
struct BorrowError<'a>(&'a u8);
impl<'a> Parser<&'a u8> for Parse { type Error = BorrowError<'a>; }
impl<'a> Error<&'a u8> for BorrowError<'a> {
    fn from_input(input: &'a u8) -> Self { Self(input) }
}
fn convert<I, E, F>(input: I) -> E
where F: Parser<I, Error = E>, E: Error<I> {
    <F as Parser<I>>::Error::from_input(input)
}
fn forward<'a, F>(f: F) -> impl Fn(&'a [u8]) -> &'a [u8]
where F: Fn(&'a [u8]) -> &'a [u8] {
    move |input: &'a [u8]| f(input)
}
fn identity(input: &[u8]) -> &[u8] { input }
fn main() {
    let value = 37;
    let error = convert::<_, BorrowError<'_>, Parse>(&value);
    let bytes = [1u8, 2, 3];
    let callback = forward(identity);
    println!("{} {}", *error.0, callback(&bytes).len());
}
