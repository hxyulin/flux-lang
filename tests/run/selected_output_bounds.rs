trait Bound {
    fn value() -> u32;
}
struct Value;
impl Bound for Value {
    fn value() -> u32 {
        29
    }
}
trait Key {
    type Out: Bound;
}
struct Leaf;
impl Key for Leaf {
    type Out = Value;
}
trait Inner {
    type I;
}
struct Wrap<T>(T);
impl<T> Inner for Wrap<T> {
    type I = T;
}
impl<T: Inner> Key for T
where
    T::I: Key,
{
    type Out = <T::I as Key>::Out;
}
type Out<T> = <T as Key>::Out;
struct Array<S: Bound>(S);
fn selected<T: Inner>(_: Array<Out<T>>) -> u32
where
    T::I: Key,
{
    <<T as Key>::Out as Bound>::value()
}
struct Data<'a, 'b>(&'a u32, &'b u32);
fn apply<'a>(x: &'a u32, f: impl for<'b> FnOnce(&mut Data<'a, 'b>) -> u32) -> u32 {
    let y = 17;
    f(&mut Data(x, &y))
}
fn main() {
    let first = selected::<Wrap<Leaf>>(Array(Value));
    let second = apply(&first, |data| *data.0 + *data.1);
    assert_eq!(first, 29);
    assert_eq!(second, 46);
    println!("{} {}", first, second);
}
