trait AsItem<T> {
    fn into_item(self) -> T;
}
impl<T> AsItem<T> for T {
    fn into_item(self) -> T { self }
}

trait Source<'a> {
    type Item: PartialEq + 'a;
    fn adapt<V: AsItem<Self::Item>>(value: V) -> Self::Item;
}

struct Wrap<'a, S: Source<'a>>(S, core::marker::PhantomData<&'a ()>);

impl<'a> Source<'a> for &'a str {
    type Item = &'a str;
    fn adapt<V: AsItem<Self::Item>>(value: V) -> Self::Item { value.into_item() }
}

impl<'a, S: Source<'a>> Source<'a> for Wrap<'a, S> {
    type Item = S::Item;
    fn adapt<V: AsItem<Self::Item>>(value: V) -> Self::Item { value.into_item() }
}

fn main() {
    let local = String::from("some text");
    println!("{}", Wrap::<&str>::adapt(local.as_str()));
}
