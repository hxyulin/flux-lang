trait Family<'a> {
    type Output: 'a;
    fn value(&'a self) -> Self::Output;
}
struct Model(String);
impl<'a> Family<'a> for Model {
    type Output = &'a str;
    fn value(&'a self) -> Self::Output {
        &self.0
    }
}
fn equal<Y: for<'a> Family<'a>>(a: &Y, b: &Y) -> bool
where
    for<'a> <Y as Family<'a>>::Output: PartialEq,
{
    a.value() == b.value()
}
struct Number(u32);
impl<'a> Family<'a> for Number {
    type Output = u32;
    fn value(&'a self) -> Self::Output {
        self.0
    }
}
struct Pair(String, String);
impl<'a> Family<'a> for Pair {
    type Output = (&'a str, &'a str);
    fn value(&'a self) -> Self::Output {
        (&self.0, &self.1)
    }
}
trait StaticFamily<'a> {
    type Output: 'static;
}
impl<'a> StaticFamily<'a> for Model {
    type Output = u32;
}
fn keep<T: 'static>(value: T) -> T {
    value
}
fn static_output<'a, Y: StaticFamily<'a>>(value: Y::Output) -> Y::Output {
    keep(value)
}
struct SelectedOwner<T>(T);
impl<'a, T: 'static> StaticFamily<'a> for SelectedOwner<T> {
    type Output = u32;
}
type Selected<T> = <SelectedOwner<T> as StaticFamily<'static>>::Output;
trait Mark<R> {
    fn number() -> u32;
}
impl Mark<u32> for u8 {
    fn number() -> u32 {
        37
    }
}
fn selected_bound<T: 'static>() -> u32
where
    u8: Mark<Selected<T>>,
{
    <u8 as Mark<u32>>::number()
}
fn main() {
    assert_eq!(selected_bound::<String>(), 37);
    let a = Model(String::from("same"));
    let b = Model(String::from("same"));
    let c = Model(String::from("other"));
    assert!(equal(&a, &b));
    assert!(!equal(&a, &c));
    assert!(equal(&Number(3), &Number(3)));
    assert!(!equal(&Number(3), &Number(5)));
    let pair = Pair(String::from("left"), String::from("right"));
    let same_pair = Pair(String::from("left"), String::from("right"));
    let other_pair = Pair(String::from("left"), String::from("other"));
    assert!(equal(&pair, &same_pair));
    assert!(!equal(&pair, &other_pair));
    let output = static_output::<Model>(17);
    assert_eq!(output, 17);
    println!("{} {} {}", equal(&a, &b), equal(&a, &c), output);
}
