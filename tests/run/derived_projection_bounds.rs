use std::fmt::Debug;

trait Source {
    type Item;
}

#[derive(Debug, Clone)]
struct Leaf(u32);
impl Source for Leaf {
    type Item = u32;
}

#[derive(Debug, Clone)]
struct Nested;
impl Source for Nested {
    type Item = Leaf;
}

macro_rules! define {
    ($($item:tt)*) => { #[derive(Debug, Clone)] $($item)* };
}

define! {
    struct Wrap<T: Source>
    where
        T::Item: Source,
        <T::Item as Source>::Item: Copy + Debug,
    {
        source: T,
        item: T::Item,
        value: <T::Item as Source>::Item,
    }
}

fn main() {
    let value = Wrap { source: Nested, item: Leaf(7), value: 19 };
    let cloned = value.clone();
    println!("{:?} {} {}", cloned, cloned.item.0, cloned.value);
}
