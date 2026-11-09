trait Read {
    fn read(self) -> u32;
}

impl Read for &u32 {
    fn read(self) -> u32 { *self }
}

fn read<T>(value: &T) -> u32
where
    for<'a> &'a T: Read,
{
    value.read()
}

fn forward<T>(value: &T) -> u32
where
    for<'b> &'b T: Read,
{
    read(value)
}

trait Pair<T> { fn sum(self, other: T) -> u32; }
impl<'a> Pair<&'a u32> for &'a u32 {
    fn sum(self, other: &'a u32) -> u32 { *self + *other }
}

fn pair<T>(left: &T, right: &T) -> u32
where
    for<'a> &'a T: Pair<&'a T>,
{
    left.sum(right)
}

fn main() {
    let first = 17;
    let second = 25;
    assert_eq!(forward(&first), 17);
    assert_eq!(pair(&first, &second), 42);
    println!("{} {}", forward(&first), pair(&first, &second));
}
