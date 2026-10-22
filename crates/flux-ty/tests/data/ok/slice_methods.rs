// `core`'s slice methods: their results' types, and their iterators'.

use core::cmp::Ordering;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Card {
    rank: u8,
}

fn main() {
    let mut a = [5, 3, 8, 1];
    a.swap(0, 3);
    a.reverse();
    a.rotate_left(1);
    a.rotate_right(1);
    let (left, right) = a.split_at(2);
    let (l, r) = a.split_at_mut(1);
    l[0] = r[0];
    let first = a.first_mut();
    let last = a.last_mut();
    let item = a.get_mut(1);
    let split = a.split_first();
    let mut b = [0; 4];
    b.copy_from_slice(&a);
    b.clone_from_slice(&a);
    b.fill(7);
    let has = a.contains(&3);
    let starts = a.starts_with(&[1, 3]);
    let ends = a.ends_with(&b[2..]);
    let windows = a.windows(2);
    let chunks = a.chunks(3).rev();
    let exact = a.chunks_exact(3).remainder();
    let mutable = b.chunks_mut(2);
    let found: Result<usize, usize> = a.binary_search(&5);
    let by = a.binary_search_by(|x| x.cmp(&5));
    let by_key = a.binary_search_by_key(&10, |x| x * 2);
    a.sort();
    a.sort_unstable();
    a.sort_by(|x, y| y.cmp(x));
    a.sort_unstable_by(|x, y| x.cmp(y));
    let mut cards = [Card { rank: 2 }, Card { rank: 1 }];
    cards.sort_by_key(|c| c.rank);
    cards.sort_unstable_by_key(|c| c.rank);
    let sorted = cards.is_sorted();
    let position = a.iter().rposition(|x| *x > 2);
    let len = a.iter().len();
    let hint = a.iter_mut().size_hint();
    let equal = a[..] == b[..];
    let less = a[..2] < b[..2];
    let order: Ordering = a[..].cmp(&b[..]);
}
