use std::sync::atomic::{AtomicUsize, Ordering};

static DROPPED: AtomicUsize = AtomicUsize::new(0);

struct Owned(usize);

impl Drop for Owned {
    fn drop(&mut self) {
        DROPPED.fetch_add(self.0, Ordering::Relaxed);
    }
}

enum Entry {
    Single(Owned),
    Pair(Owned, Owned),
    Other(Owned),
}

fn take(entries: &mut Vec<(usize, Entry, usize)>) -> Owned {
    match entries.pop() {
        Some((_, Entry::Single(value), _)) => value,
        _ => panic!("expected a single entry"),
    }
}

fn take_partial(entries: &mut Vec<(usize, Entry, usize)>) -> Option<Owned> {
    match entries.pop() {
        Some((_, Entry::Pair(value, _), _)) => Some(value),
        _ => None,
    }
}

fn take_known_pair(entry: Entry) -> Owned {
    match entry {
        Entry::Pair(value, _) => value,
        _ => panic!("expected a pair"),
    }
}

fn main() {
    let mut entries = vec![
        (0, Entry::Other(Owned(16)), 0),
        (0, Entry::Pair(Owned(4), Owned(8)), 0),
        (0, Entry::Single(Owned(2)), 0),
    ];
    let value = take(&mut entries);
    assert_eq!(DROPPED.load(Ordering::Relaxed), 0);
    drop(value);
    let value = take_partial(&mut entries).unwrap();
    assert_eq!(DROPPED.load(Ordering::Relaxed), 10);
    drop(value);
    assert!(take_partial(&mut entries).is_none());
    assert!(take_partial(&mut entries).is_none());
    assert_eq!(DROPPED.load(Ordering::Relaxed), 30);
    let value = take_known_pair(Entry::Pair(Owned(64), Owned(128)));
    assert_eq!(DROPPED.load(Ordering::Relaxed), 158);
    drop(value);
    assert_eq!(DROPPED.load(Ordering::Relaxed), 222);

    #[cfg(panic = "unwind")]
    {
        std::panic::set_hook(Box::new(|_| {}));
        let mismatch = std::panic::catch_unwind(|| {
            take(&mut vec![(0, Entry::Other(Owned(32)), 0)]);
        });
        assert!(mismatch.is_err());
        assert!(std::panic::catch_unwind(|| take(&mut Vec::new())).is_err());
        assert_eq!(DROPPED.load(Ordering::Relaxed), 254);
    }
    println!("verified");
}
