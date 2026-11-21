type Items<'a, T> = Box<dyn Iterator<Item = T> + 'a>;

trait Source<'a> {
    type Item: 'a;
    fn items(&self) -> Items<'a, Self::Item>;
}

impl<'a> Source<'a> for &'a str {
    type Item = &'a str;

    fn items(&self) -> Items<'a, Self::Item> {
        Box::new(std::iter::once(*self))
    }
}

struct Collected<'a, S: Source<'a>>
where
    S::Item: 'a,
{
    item: S::Item,
}

fn first<'a, S: Source<'a>>(source: &S) -> Collected<'a, S> {
    Collected { item: source.items().next().unwrap() }
}

fn collect<'a, S: Source<'a>>(source: &S) -> Items<'a, Option<S::Item>> {
    Box::new(source.items().map(|item| Some(item)))
}

fn main() {
    let text = String::from("borrowed");
    let borrowed = text.as_str();
    let mut collected = collect(&borrowed);
    println!("{}", collected.next().unwrap().unwrap());
    assert!(collected.next().is_none());
    assert_eq!(first(&borrowed).item, borrowed);
}
