//! `core`'s slice methods, on arrays and slices: swapping, reversing,
//! rotating, splitting, copying, filling, searching (`contains`,
//! `starts_with`, `binary_search` on duplicates, which finds the element
//! rustc's does), windows and chunks from both ends, `first_mut` and
//! friends, stable sorts by key and comparator on many equal keys,
//! unstable sorts, `rposition` and `len` of slice iterators, slice
//! comparisons, and `fill` and `swap` on values with `Clone` and `Drop`.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Card {
    rank: u8,
    suit: u8,
}

struct Noisy(i32);

impl Clone for Noisy {
    fn clone(&self) -> Noisy {
        unsafe { printf(c"clone %d\n".as_ptr(), self.0) };
        Noisy(self.0 + 100)
    }
}

impl Drop for Noisy {
    fn drop(&mut self) {
        unsafe { printf(c"drop %d\n".as_ptr(), self.0) };
    }
}

fn show(v: &[i64]) {
    for x in v {
        unsafe { printf(c"%lld ".as_ptr(), *x) };
    }
    unsafe { printf(c"\n".as_ptr()) };
}

fn next(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

fn main() {
    let mut a = [5i64, 3, 8, 1, 9, 2];
    a.swap(0, 5);
    show(&a);
    a.reverse();
    show(&a);
    a.rotate_left(2);
    show(&a);
    a.rotate_right(1);
    show(&a);
    let (l, r) = a.split_at(2);
    show(l);
    show(r);
    {
        let (l, r) = a.split_at_mut(3);
        l[0] = r[0] * 10;
        r[2] = l[1];
    }
    show(&a);
    let mut b = [0i64; 6];
    b.copy_from_slice(&a);
    b[..3].fill(7);
    show(&b);
    unsafe {
        printf(
            c"%d %d %d %d %d\n".as_ptr(),
            a.contains(&9) as i32,
            a.contains(&4) as i32,
            a.starts_with(&a[..2]) as i32,
            a.ends_with(&[1, 2]) as i32,
            (a[..] == b[..]) as i32,
        );
    }
    for w in a.windows(3) {
        unsafe { printf(c"[%lld %lld %lld] ".as_ptr(), w[0], w[1], w[2]) };
    }
    unsafe { printf(c"\n".as_ptr()) };
    for c in a.chunks(4) {
        unsafe { printf(c"(%zu) ".as_ptr(), c.len()) };
    }
    for c in a.chunks(4).rev() {
        unsafe { printf(c"<%zu> ".as_ptr(), c.len()) };
    }
    unsafe { printf(c"%zu\n".as_ptr(), a.chunks_exact(4).remainder().len()) };
    for c in b.chunks_mut(4) {
        c[0] = -1;
    }
    show(&b);
    if let Some(x) = a.first_mut() {
        *x = 42;
    }
    if let Some(x) = a.last_mut() {
        *x += 1;
    }
    if let Some(x) = a.get_mut(2) {
        *x = 0;
    }
    show(&a);
    a.sort();
    show(&a);
    let dup = [1, 2, 2, 2, 2, 3, 5, 5, 8, 8, 8, 8, 9];
    let mut i = 0;
    while i < 11 {
        let found = match dup.binary_search(&i) {
            Ok(k) => k as i64,
            Err(k) => -(k as i64) - 1,
        };
        unsafe { printf(c"%lld ".as_ptr(), found) };
        i += 1;
    }
    unsafe { printf(c"\n".as_ptr()) };
    let pos = a.iter().position(|&x| x > 5);
    let rpos = a.iter().rposition(|&x| x > 5);
    unsafe { printf(c"%zu %zu %zu %zu\n".as_ptr(), pos.unwrap(), rpos.unwrap(), a.iter().len(), a.iter().size_hint().0) };

    // Stable sorts on many elements with equal keys.
    let mut state = 88172645463325252u64;
    let mut cards = [Card { rank: 0, suit: 0 }; 100];
    for c in cards.iter_mut() {
        *c = Card { rank: (next(&mut state) % 13) as u8, suit: (next(&mut state) % 4) as u8 };
    }
    let mut by_rank = cards;
    by_rank.sort_by_key(|c| c.rank);
    let mut by_suit = cards;
    by_suit.sort_by(|x, y| y.suit.cmp(&x.suit));
    let mut full = cards;
    full.sort_unstable();
    let mut small = [Card { rank: 3, suit: 1 }, Card { rank: 1, suit: 2 }, Card { rank: 3, suit: 0 }, Card { rank: 1, suit: 1 }];
    small.sort_unstable_by_key(|c| c.rank);
    let mut h = 0u64;
    for c in by_rank.iter().chain(by_suit.iter()).chain(full.iter()).chain(small.iter()) {
        h = h.wrapping_mul(31).wrapping_add((c.rank as u64) * 4 + c.suit as u64);
    }
    unsafe { printf(c"%llu %d\n".as_ptr(), h, full.is_sorted() as i32) };
    let mut big = [0i64; 1000];
    for x in big.iter_mut() {
        *x = (next(&mut state) % 1000) as i64;
    }
    big.sort_unstable_by(|x, y| y.cmp(x));
    let mut sum = 0i64;
    for (i, x) in big.iter().enumerate() {
        sum = sum.wrapping_mul(7).wrapping_add(*x ^ i as i64);
    }
    unsafe { printf(c"%lld %lld %lld\n".as_ptr(), sum, big[0], big[999]) };
    let less = [1, 2, 3][..] < [1, 3][..];
    let cmp = [1, 2][..].cmp(&[1, 2, 0][..]);
    unsafe { printf(c"%d %d\n".as_ptr(), less as i32, cmp as i32) };

    let mut noisy = [Noisy(1), Noisy(2), Noisy(3)];
    noisy.fill(Noisy(9));
    noisy.swap(0, 2);
    unsafe { printf(c"end\n".as_ptr()) };
}
