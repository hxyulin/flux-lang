// A bound on Cow's abstract owned type can use a nested ToOwned assumption.
use std::borrow::Cow;
use std::fmt::Debug;

#[derive(Clone, Debug)]
struct Entry<'a, T>
where
    [T]: ToOwned,
    <[T] as ToOwned>::Owned: Debug,
{
    data: Cow<'a, [T]>,
}

struct Descriptor<'a, T>
where
    <[Entry<'a, T>] as ToOwned>::Owned: Debug,
    [Entry<'a, T>]: ToOwned,
    <[T] as ToOwned>::Owned: Debug,
    [T]: ToOwned,
{
    entries: Cow<'a, [Entry<'a, T>]>,
}

fn main() {
    let values = [17_u32, 25];
    let entries = [Entry { data: Cow::Borrowed(&values[..]) }];
    let descriptor = Descriptor { entries: Cow::Borrowed(&entries[..]) };
    println!("{}", descriptor.entries[0].data[0] + descriptor.entries[0].data[1]);
}
