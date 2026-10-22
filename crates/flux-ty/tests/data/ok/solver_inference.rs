struct S;
trait Choose<T> {}
impl Choose<u8> for S {}
impl Choose<u16> for S {}
fn choose<T>(_: &S, value: T) -> T
where
    S: Choose<T>,
{
    value
}
fn main() {
    let a = choose(&S, 1);
    let _: u8 = a;
    let b = choose(&S, 2);
    let _: u16 = b;
}
