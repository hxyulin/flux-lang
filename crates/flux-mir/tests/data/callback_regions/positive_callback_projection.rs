fn first<I: Iterator<Item = &'static i32>>(mut x: I) -> Option<&'static i32> { x.next() }
struct Wrap<I>(I);
impl<I: Iterator<Item = &'static i32>> Wrap<I> {
    fn first(mut x: I) -> Option<&'static i32> { x.next() }
}
fn main() {
    static N: i32 = 42;
    println!("{}", [[&N].into_iter()].into_iter().find_map(first).unwrap());
    println!("{}", [[&N].into_iter()].into_iter().find_map(Wrap::first).unwrap());
}
