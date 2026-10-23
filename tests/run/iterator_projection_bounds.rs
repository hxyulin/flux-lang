// A declaration bound and an explicit supertrait bound supply one proof.
fn consume<I: IntoIterator<Item = u32>>(input: I) -> u32
where
    I::IntoIter: ExactSizeIterator,
{
    let values = input.into_iter();
    if values.len() == 0 {
        assert_eq!(values.count(), 0);
        return 0;
    }
    let mut sum = 0;
    for value in values {
        let value: u32 = value;
        sum += value;
    }
    sum
}

fn borrowed<'a, I: IntoIterator<Item = &'a u32>>(input: I) -> u32
where
    I::IntoIter: ExactSizeIterator,
{
    let values = input.into_iter();
    if values.len() == 0 {
        assert_eq!(values.count(), 0);
        return 0;
    }
    let mut sum = 0;
    for value in values {
        let value: &u32 = value;
        sum += *value;
    }
    sum
}

fn main() {
    assert_eq!(generic(Holder(Part(42))), 42);
    let values = [4, 9, 16];
    println!(
        "{} {} {} {}",
        consume([1, 2]),
        consume([]),
        borrowed(&values),
        borrowed(&[])
    );
}

trait Value {
    fn value(&self) -> u32;
}
trait More: Value {}
trait Container {
    type Part: Value;
    fn part(self) -> Self::Part;
}
struct Part(u32);
impl Value for Part {
    fn value(&self) -> u32 {
        self.0
    }
}
impl More for Part {}
struct Holder(Part);
impl Container for Holder {
    type Part = Part;
    fn part(self) -> Part {
        self.0
    }
}
fn generic<C: Container>(c: C) -> u32
where
    C::Part: More,
{
    c.part().value()
}
