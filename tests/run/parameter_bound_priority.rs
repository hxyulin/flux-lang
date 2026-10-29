trait Typed: Clone {
    type Value;
    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = u8> + '_>>;
}
trait Erased {
    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = u8> + '_>>;
}
impl<T, P> Erased for P
where
    P: Typed<Value = T>,
{
    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = u8> + '_>> {
        P::possible_values(self)
    }
}
#[derive(Clone)]
struct Parser {
    values: [u8; 3],
}
impl Typed for Parser {
    type Value = u32;
    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = u8> + '_>> {
        Some(Box::new(self.values.iter().copied()))
    }
}
fn generic<P: Typed>(parser: &P) -> Vec<u8> {
    P::possible_values(parser).unwrap().collect()
}
trait Number {
    const VALUE: u32;
    fn number() -> u32;
}
trait Other {
    const VALUE: u32;
    fn number() -> u32;
}
impl<T: Number> Other for T {
    const VALUE: u32 = 91;
    fn number() -> u32 {
        97
    }
}
impl Number for Parser {
    const VALUE: u32 = 13;
    fn number() -> u32 {
        17
    }
}
fn numbers<T: Number>() -> u32 {
    T::VALUE + T::number()
}
trait Mutable {
    fn read(&mut self) -> u32;
}
trait Shared {
    fn read(&self) -> u32;
}
impl<T: Mutable> Shared for T {
    fn read(&self) -> u32 {
        23
    }
}
impl Mutable for Parser {
    fn read(&mut self) -> u32 {
        29
    }
}
fn shared<T: Mutable>(value: &T) -> u32 {
    value.read()
}
fn main() {
    let parser = Parser { values: [3, 5, 7] };
    assert_eq!(generic(&parser), vec![3, 5, 7]);
    let erased: &dyn Erased = &parser;
    assert_eq!(
        erased.possible_values().unwrap().collect::<Vec<_>>(),
        vec![3, 5, 7]
    );
    assert_eq!(numbers::<Parser>(), 30);
    assert_eq!(shared(&parser), 23);
    println!(
        "bound priority {:?} {} {}",
        generic(&parser),
        numbers::<Parser>(),
        shared(&parser)
    );
}
