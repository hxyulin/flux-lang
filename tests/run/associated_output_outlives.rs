trait Output {
    type Value: 'static;
    fn value(&self) -> Self::Value;
}

struct Borrowed<'a>(&'a u32);
impl Output for Borrowed<'_> {
    type Value = u32;
    fn value(&self) -> u32 {
        *self.0
    }
}

fn require_static<T: 'static>(value: &T) -> &T {
    value
}

fn inspect<T: Output>(owner: &T) -> T::Value {
    let value = owner.value();
    let _ = require_static(&value);
    value
}

trait Static: 'static {}
impl Static for [u32; 2] {}
trait Base {
    type Value: Static;
    fn value(&self) -> Self::Value;
}
trait Derived: Base {}
impl Base for Borrowed<'_> {
    type Value = [u32; 2];
    fn value(&self) -> Self::Value {
        [*self.0, *self.0 + 1]
    }
}
impl Derived for Borrowed<'_> {}

fn inherited<T: Derived>(owner: &T) -> T::Value {
    let value = owner.value();
    let _ = require_static(&value);
    value
}

trait ArraySize {
    type Array<T>;
}
struct Two;
impl ArraySize for Two {
    type Array<T> = [T; 2];
}
struct Array<T, U: ArraySize>(U::Array<T>);
fn array_view<T, U: ArraySize>(array: &Array<T, U>) -> &U::Array<T> {
    &array.0
}

fn main() {
    let number = 41;
    let owner = Borrowed(&number);
    let first = inspect(&owner);
    let pair = inherited(&owner);
    assert_eq!(first, 41);
    assert_eq!(pair, [41, 42]);
    let array = Array::<_, Two>([&number, &pair[1]]);
    let view = array_view(&array);
    assert_eq!(**view.first().unwrap(), 41);
    assert_eq!(**view.last().unwrap(), 42);
    println!("associated output {first} {} {}", pair[1], view.len());
}
