use std::sync::atomic::{AtomicUsize, Ordering};

static DROPS: AtomicUsize = AtomicUsize::new(0);

struct Counted(usize);

impl Drop for Counted {
    fn drop(&mut self) {
        DROPS.fetch_add(self.0, Ordering::Relaxed);
    }
}

enum Entry {
    Number(usize),
    Wide([usize; 32]),
    Owned(Box<Counted>),
}

#[inline(never)]
fn take(entries: &mut Vec<Entry>) -> Option<usize> {
    match entries.pop()? {
        Entry::Number(value) => Some(value),
        Entry::Wide(words) => Some(words[31]),
        Entry::Owned(value) => {
            let code = value.0;
            drop(value);
            Some(code)
        }
    }
}

#[inline(never)]
fn replace_top(entries: &mut Vec<Entry>, replacement: Entry) -> Option<usize> {
    let old = entries.pop();
    entries.push(replacement);
    match old? {
        Entry::Number(value) => Some(value),
        Entry::Wide(words) => Some(words[31]),
        Entry::Owned(value) => Some(value.0),
    }
}

#[inline(never)]
fn consume(entry: Option<Entry>) -> usize {
    match entry {
        None => 0,
        Some(Entry::Number(value)) => value,
        Some(Entry::Wide(words)) => words[31],
        Some(Entry::Owned(value)) => value.0,
    }
}

#[inline(never)]
fn pass_by_value(entries: &mut Vec<Entry>) -> usize {
    consume(entries.pop())
}

fn main() {
    let mut entries = Vec::with_capacity(4);
    assert_eq!(take(&mut entries), None);
    entries.push(Entry::Wide([11; 32]));
    entries.push(Entry::Owned(Box::new(Counted(17))));
    assert_eq!(take(&mut entries), Some(17));
    assert_eq!(DROPS.load(Ordering::Relaxed), 17);
    assert_eq!(replace_top(&mut entries, Entry::Number(7)), Some(11));
    assert_eq!(pass_by_value(&mut entries), 7);
    assert_eq!(pass_by_value(&mut entries), 0);
    assert_eq!(entries.len(), 0);
    println!("{}", DROPS.load(Ordering::Relaxed));
}
