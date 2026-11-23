struct Entry<'a> {
    text: &'a str,
    value: usize,
}

struct Selection<'a> {
    name: Option<&'a str>,
}

struct Index<'a>(Vec<Option<Selection<'a>>>);

impl<'a> Index<'a> {
    fn new(entries: &'a [Entry<'a>]) -> Self {
        Self(
            entries
                .iter()
                .map(|entry| Some(Selection { name: Some(entry.text) }))
                .collect(),
        )
    }
}

fn update(entries: &mut [Entry<'_>]) {
    let index = Index::new(entries);
    let mut edits = Vec::new();
    for (position, selected) in index.0.iter().enumerate() {
        if let Some(Selection { name: Some(name) }) = selected {
            edits.push((position, name.len()));
        }
    }
    for (position, len) in edits {
        entries[position].value = len;
    }
}

fn main() {
    let text = String::from("abc");
    let mut entries = [Entry { text: &text, value: 0 }];
    update(&mut entries);
    println!("{}", entries[0].value);
}
