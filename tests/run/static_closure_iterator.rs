//! Iterator item lifetimes are proved inside closures defined in statics.

use std::sync::LazyLock;

static WORDS: LazyLock<Vec<String>> = LazyLock::new(|| vec![String::from("one")]);

fn collect<I: IntoIterator<Item = &'static str>>(iter: I) -> Vec<&'static str> {
    iter.into_iter().collect()
}

static VIEWS: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| collect(WORDS.iter().map(String::as_str)));

fn main() {
    println!("{}", VIEWS[0]);
}
