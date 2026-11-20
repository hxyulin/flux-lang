use std::fmt::Debug;

fn borrowed<'a>(
    values: &'a [Box<dyn Debug>],
) -> Box<dyn Iterator<Item = &'a (dyn Debug + 'static)> + 'a> {
    Box::new(values.iter().map(|value| value.as_ref()))
}

fn main() {
    let values: [Box<dyn Debug>; 2] = [Box::new(42), Box::new("tuple")];
    for value in borrowed(&values) {
        println!("{value:?}");
    }
}
