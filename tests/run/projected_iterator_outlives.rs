type Items<'a, T> = Box<dyn Iterator<Item = T> + 'a>;

trait Source<'a> {
    type Item: 'a;
}
impl<'a> Source<'a> for &'a str {
    type Item = &'a str;
}

struct Expand<'a, T: 'a> {
    inner: Items<'a, T>,
}
impl<'a, T: 'a> Iterator for Expand<'a, T> {
    type Item = T;
    fn next(&mut self) -> Option<T> { self.inner.next() }
}

fn flatten<'a, S: Source<'a>>(items: Items<'a, (S::Item, Items<'a, S::Item>)>) -> Items<'a, S::Item> {
    Box::new(items.flat_map(|(_item, inner)| Expand { inner }))
}

fn main() {
    let text = String::from("from a borrowed iterator");
    let mut values = text.split(' ');
    let first = values.next().unwrap();
    let rest: Items<'_, &str> = Box::new(values);
    let values = flatten::<&str>(Box::new(std::iter::once((first, rest))));
    for value in values {
        println!("{value}");
    }
}
