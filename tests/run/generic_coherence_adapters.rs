use std::borrow::Cow;
use std::marker::PhantomData;
trait Flag<L, R> {
    fn kind(&self) -> u8;
}
struct Wrap<F, T>(F, PhantomData<T>);
impl<L, R, F: FnMut(&L, &R) -> bool> Flag<L, R> for Wrap<F, bool> {
    fn kind(&self) -> u8 {
        1
    }
}
impl<T, F: FnMut(&T, &T) -> bool> Flag<T, T> for F {
    fn kind(&self) -> u8 {
        2
    }
}
fn kind<L, R, T: Flag<L, R>>(value: &T) -> u8 {
    value.kind()
}
struct Paint<'a, S: ToOwned + ?Sized> {
    text: Cow<'a, S>,
}
impl<'a, I, S: 'a + ToOwned + ?Sized> From<I> for Paint<'a, S>
where
    I: Into<Cow<'a, S>>,
    S::Owned: std::fmt::Debug,
{
    fn from(value: I) -> Self {
        Self { text: value.into() }
    }
}
fn main() {
    let wrapped = Wrap(|a: &u8, b: &u8| a < b, PhantomData::<bool>);
    let plain = |a: &u8, b: &u8| a == b;
    assert_eq!(kind::<u8, u8, _>(&wrapped), 1);
    assert_eq!(kind::<u8, u8, _>(&plain), 2);
    let borrowed: Paint<'_, str> = "borrowed".into();
    let owned: Paint<'_, str> = String::from("owned").into();
    println!(
        "{} {} {} {}",
        kind::<u8, u8, _>(&wrapped),
        kind::<u8, u8, _>(&plain),
        borrowed.text,
        owned.text
    );
}
