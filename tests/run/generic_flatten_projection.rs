type Items<'a, T> = Box<dyn Iterator<Item = T> + 'a>;

struct Row<'a> {
    first: &'a [String],
    more: &'a [Option<Vec<String>>],
}

fn visit<'a, F: FnMut(&Row<'a>) -> Items<'a, &'a str>>(row: Row<'a>, mut f: F) {
    for value in f(&row) {
        println!("{value}");
    }
}

fn show<T>(_: T, first: &[String], more: &[Option<Vec<String>>]) {
    visit(Row { first, more }, move |row| {
        let first = row.first.iter();
        let rows = row
            .more
            .iter()
            .filter_map(|value| match value {
                Some(value) => Some(value.as_slice()),
                None => None,
            })
            .flatten();
        Box::new(first.chain(rows).filter_map(|string| {
            if string.is_empty() {
                None
            } else {
                Some(string.as_str())
            }
        }))
    });
}

fn main() {
    let first = [String::from("zero")];
    let more = [Some(vec![String::from("one"), String::from("two")]), None];
    show((), &first, &more);
}
