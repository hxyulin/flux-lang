type Items<'a, T> = Box<dyn Iterator<Item = T> + 'a>;
trait Source<'a> { type Item: 'a; }

fn resolve<'a, S: Source<'a>, F: FnMut(&S::Item) -> Items<'a, S::Item> + 'a>(
    items: Items<'a, S::Item>,
    mut callback: F,
) -> Items<'a, (S::Item, Items<'a, S::Item>)> {
    Box::new(items.map(move |item| {
        let others = callback(&item);
        (item, others)
    }))
}
impl<'a> Source<'a> for &'a str {
    type Item = &'a str;
}

fn main() {
    let text = String::from("keep this alive");
    let borrowed = text.as_str();
    let mut seen = 0;
    let items = resolve::<&str, _>(
        Box::new(std::iter::once(borrowed)),
        move |item| {
            seen += 1;
            assert_eq!(seen, 1);
            Box::new(std::iter::once(*item))
        },
    );
    for (item, others) in items {
        println!("{item}");
        for other in others {
            println!("{other}");
        }
    }
}
