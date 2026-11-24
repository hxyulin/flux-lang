const WORDS: &[&str] = &[
    "", "cat", "car", "bar", "baz", "bat", "café", "cafe", "λ", "東京", "long_keyword",
    "ÿ", "a\0b", "other", "cat",
];

fn search(items: &mut std::slice::Iter<'_, &str>, query: &&str) -> bool {
    items.any(|word| word == query)
}

#[inline(never)]
pub fn fresh(query: &str) -> bool {
    search(&mut WORDS.iter(), &query)
}

#[inline(never)]
pub fn reused(query: &str) -> (bool, Option<&'static str>) {
    let mut items = WORDS.iter();
    let found = search(&mut items, &query);
    (found, items.next().copied())
}

#[inline(never)]
pub fn repeating(query: &str) -> usize {
    let mut items = WORDS.iter();
    let mut matches = 0;
    for _ in 0..2 {
        if search(&mut items, &query) {
            matches += 1;
        }
    }
    matches
}

fn visible_search(items: &mut std::slice::Iter<'_, &str>, query: &&str) -> bool {
    items.any(|word| {
        if word == query {
            println!("found {word}");
            true
        } else {
            false
        }
    })
}

#[inline(never)]
pub fn visible(query: &str) -> bool {
    visible_search(&mut WORDS.iter(), &query)
}

fn main() {
    for query in [
        "", "cat", "car", "bar", "bat", "bad", "café", "cafè", "cafe", "λ", "μ", "東京",
        "東亰", "long_keyword", "long_keywore", "ÿ", "ŷ", "a\0b", "a_b", "other", "absent",
    ] {
        assert_eq!(fresh(query), WORDS.iter().any(|word| *word == query));
    }
    assert_eq!(reused("cat"), (true, Some("car")));
    assert_eq!(reused("absent"), (false, None));
    assert_eq!(repeating("cat"), 2);
    assert_eq!(repeating("car"), 1);
    assert!(visible("café"));
    assert!(!visible("absent"));
    println!("constant string membership verified");
}
