trait Select<T, U> {
    fn select(x: T, y: U) -> U;
}
struct Pick;
impl<T, U> Select<T, U> for Pick {
    fn select(_: T, y: U) -> U { y }
}
fn direct<'a, 'b, 'c>(x: (&'a i32, &'b i32), y: &'c i32) -> &'c i32 {
    Pick::select(x, y)
}
trait Echo<T> {
    fn echo(x: T) -> T;
}
impl<T> Echo<T> for Pick {
    fn echo(x: T) -> T { x }
}
fn callback<'a, 'b>(x: (&'a i32, &'b i32)) -> (&'a i32, &'b i32) {
    [x].into_iter().map(Pick::echo).next().unwrap()
}
fn main() {
    let x = 11;
    let y = 23;
    let z = 37;
    println!("{}", direct((&x, &y), &z));
    let pair = callback((&x, &y));
    println!("{} {}", pair.0, pair.1);
}
