trait Decode<'de> {
    fn decode(&self, input: &'de [u8]) -> u32;
}
struct Owned(u32);
impl<'de> Decode<'de> for Owned {
    fn decode(&self, input: &'de [u8]) -> u32 {
        self.0 + input.iter().map(|byte| *byte as u32).sum::<u32>()
    }
}
trait DecodeOwned: for<'de> Decode<'de> {}
impl<T> DecodeOwned for T where T: for<'de> Decode<'de> {}
trait Complete: DecodeOwned {}
impl<T: DecodeOwned> Complete for T {}
fn decode<'de, T: Decode<'de>>(value: &T, input: &'de [u8]) -> u32 {
    value.decode(input)
}
fn independently<T: Complete>(value: &T) -> u32 {
    let first = [2, 3];
    let a = decode(value, &first);
    let second = [5, 7, 11];
    a + decode(value, &second)
}
fn apply(f: impl for<'a> FnOnce(&'a u32) -> u32) -> u32 {
    f(&13)
}
fn read(value: &u32) -> u32 {
    *value + 1
}
trait Pair<'a, 'b> {}
struct Held<'b>(&'b u32);
impl<'a, 'b> Pair<'a, 'b> for Held<'b> {}
fn correlated<'b, T: for<'a> Pair<'a, 'b>>(_: T, input: &'b u32) -> &'b u32 {
    input
}
fn main() {
    let result = independently(&Owned(17));
    let local = 29;
    let held = Held(&local);
    assert_eq!(*held.0, local);
    let borrowed = correlated(held, &local);
    assert_eq!(*borrowed, 29);
    assert_eq!(result, 62);
    assert_eq!(apply(read), 14);
    println!("higher-ranked {} {} {}", result, apply(read), borrowed);
}
