trait Convert {
    fn make<T>(input: T) -> Self;
}
trait Decode<'de> {
    type Error: Convert;
}
impl Convert for u32 {
    fn make<T>(_: T) -> Self {
        7
    }
}
impl<'de> Decode<'de> for &'de () {
    type Error = u32;
}

fn convert<'de, D: Decode<'de>>(input: Result<u8, u16>) -> Result<u8, D::Error> {
    input.map_err(<<D as Decode<'de>>::Error as Convert>::make)
}

fn main() {
    assert_eq!(convert::<&()>(Err(2)), Err(7));
}
