//! `Zip` as rustc specializes it, seen through closures that print what
//! they see: pairs of iterators with random access (slices', ranges',
//! `str::bytes` and chunks', and `map`, `enumerate`, `copied`, `cloned`,
//! `skip` and `fuse` of them) are got by index, so `next`, `nth`,
//! `next_back` and `fold` never take an item the other iterator cannot
//! pair; `fold` (and `sum`, `count`, `for_each`, `last`, `max`, ...) of
//! iterators that know their lengths (`rev`, `chain`, `take`, arrays'
//! `into_iter`, inclusive and 128-bit ranges, `by_ref`, ...) takes as
//! many pairs as the shorter has; other iterators (`filter`, `inspect`)
//! give an item before `Zip` finds the other empty. Adapters' `fold`s
//! reach the `Zip`'s, and `Iterator::eq` of iterators that know their
//! lengths compares no items when the lengths differ.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn see(tag: i32, x: i32) -> i32 {
    unsafe { printf(c"  saw %d %d\n".as_ptr(), tag, x) };
    x
}

fn case(n: i32) {
    unsafe { printf(c"case %d\n".as_ptr(), n) };
}

fn show(label: i32, value: i64) {
    unsafe { printf(c"  %d: %lld\n".as_ptr(), label, value) };
}

fn hint(h: (usize, Option<usize>)) {
    let upper = match h.1 {
        Some(n) => n as i64,
        None => -1,
    };
    unsafe { printf(c"  hint %lld %lld\n".as_ptr(), h.0 as i64, upper) };
}

struct Noisy(i32);

impl Clone for Noisy {
    fn clone(&self) -> Noisy {
        unsafe { printf(c"  clone %d\n".as_ptr(), self.0) };
        Noisy(self.0)
    }
}

fn main() {
    let a = [1, 2, 3, 4, 5];
    let b = [10, 20, 30];

    case(1); // The reproducer: `fold` of a mapped slice iterator.
    let s = a.iter().map(|x| see(1, *x)).zip(0..2).fold(0, |acc, (x, y)| acc + x * y);
    show(1, s as i64);

    case(2); // `fold` of a reversed array iterator, which knows its length.
    let t = a.into_iter().rev().map(|x| see(2, x)).zip(0..0).fold(0, |acc, (x, _)| acc + x);
    show(2, t as i64);

    case(3); // `sum` of a mapped range.
    let u: i32 = (10..13).map(|x| see(3, x)).zip(0..1).map(|(x, _)| x).sum();
    show(3, u as i64);

    case(4); // `for` loops: random access never takes the unpaired item.
    for (x, y) in a.iter().map(|x| see(4, *x)).zip(b.iter()) {
        show(4, (x * y) as i64);
    }

    case(5); // `next` of a reversed iterator does take it.
    for (x, y) in a.iter().rev().map(|x| see(5, *x)).zip(b.iter()) {
        show(5, (x * y) as i64);
    }

    case(6); // Nor does `fold` of a filter, which does not know its length.
    let n = a.iter().filter(|x| see(6, **x) > 1).zip(0..2).count();
    show(6, n as i64);

    case(7); // `inspect` neither.
    let n = a.iter().inspect(|x| { see(7, **x); }).zip(0..2).count();
    show(7, n as i64);

    case(8); // `size_hint` and `len` as pairs are taken.
    let mut z = a.iter().map(|x| see(8, *x)).zip(b.iter());
    hint(z.size_hint());
    z.next();
    hint(z.size_hint());
    show(8, z.len() as i64);
    let mut z = a.iter().filter(|_| true).zip(b.iter());
    hint(z.size_hint());
    z.next();
    hint(z.size_hint());

    case(9); // `nth` gets the pairs it skips for their effects.
    let mut z = a.iter().map(|x| see(9, *x)).zip(b.iter().map(|x| see(90, *x)));
    if let Some((x, y)) = z.nth(1) {
        show(9, (x + y) as i64);
    }
    show(9, z.nth(5).is_none() as i64);
    show(9, z.next().is_none() as i64);

    case(10); // `next_back` of random access gives up the longer's extra items.
    let mut z = a.iter().map(|x| see(10, *x)).zip(b.iter().map(|x| see(100, *x)));
    while let Some((x, y)) = z.next_back() {
        show(10, (x + y) as i64);
    }

    case(11); // ... and after `next`, and once the pairs run out.
    let mut z = a.iter().map(|x| see(11, *x)).zip(b.iter());
    z.next();
    if let Some((x, _)) = z.next_back() {
        show(11, x as i64);
    }
    z.next();
    show(11, z.next_back().is_none() as i64);
    show(11, z.next().is_none() as i64);

    case(12); // `next_back` without random access.
    let mut z = a.into_iter().map(|x| see(12, x)).zip(b.into_iter().map(|x| see(120, x)));
    z.next();
    while let Some((x, y)) = z.next_back() {
        show(12, (x + y) as i64);
    }

    case(13); // Exhausted by `next`, then `next_back`, without random access.
    let mut z = a.into_iter().map(|x| see(13, x)).zip(b.into_iter());
    while let Some((x, _)) = z.next() {
        show(13, x as i64);
    }
    show(13, z.next_back().is_none() as i64);

    case(14); // Exhausted by `next`, then `next_back`, with random access.
    let mut z = a.iter().map(|x| see(14, *x)).zip(b.iter());
    while let Some((x, _)) = z.next() {
        show(14, x as i64);
    }
    show(14, z.next_back().is_none() as i64);

    case(15); // Consumers that fold: `count`, `last`, `for_each`, `product`.
    let r = || a.iter().rev().map(|x| see(15, *x));
    show(15, r().zip(0..2).count() as i64);
    if let Some((x, y)) = r().zip(0..2).last() {
        show(15, (x * 100 + y) as i64);
    }
    r().zip(0..2).for_each(|(x, y)| show(15, (x * y) as i64));
    let p: i32 = r().zip(1..3).map(|(x, y)| x * y).product();
    show(15, p as i64);

    case(16); // `max`, `min` and their `_by` and `_by_key` forms.
    let r = || a.iter().rev().map(|x| see(16, *x)).zip(0..3);
    if let Some((x, _)) = r().max() {
        show(16, x as i64);
    }
    if let Some((x, _)) = r().min() {
        show(16, x as i64);
    }
    if let Some((x, _)) = r().max_by(|p, q| p.0.cmp(&q.0)) {
        show(16, x as i64);
    }
    if let Some((x, _)) = r().min_by_key(|p| p.1) {
        show(16, x as i64);
    }
    if let Some((x, _)) = r().max_by_key(|p| p.0 % 2) {
        show(16, x as i64);
    }
    if let Some(x) = r().map(|p| p.0).reduce(|x, y| x + y) {
        show(16, x as i64);
    }

    case(17); // `by_ref`: `fold` leaves the unpaired item, `for` takes it.
    let mut it = a.iter().rev().map(|x| see(17, *x));
    let s: i32 = it.by_ref().zip(0..2).map(|(x, _)| x).sum();
    show(17, s as i64);
    show(17, it.len() as i64);
    for (x, _) in it.by_ref().zip(0..1) {
        show(17, x as i64);
    }
    show(17, it.len() as i64);

    case(18); // Chains, takes, skips, steps and peeks know their lengths.
    let s: i32 = (0..2).chain(5..7).map(|x| see(18, x)).zip(0..3).map(|(x, _)| x).sum();
    show(18, s as i64);
    let n = a.iter().rev().map(|x| see(180, *x)).take(4).zip(0..2).count();
    show(18, n as i64);
    let n = a.iter().map(|x| see(181, *x)).skip(1).zip(0..2).count();
    show(18, n as i64);
    let n = a.iter().map(|x| see(182, *x)).step_by(2).zip(0..1).count();
    show(18, n as i64);
    let mut pk = a.iter().rev().map(|x| see(183, *x)).peekable();
    if let Some(x) = pk.peek() {
        show(18, *x as i64);
    }
    show(18, pk.zip(0..2).count() as i64);

    case(19); // `skip` with random access gets the items it skips.
    let mut z = a.iter().map(|x| see(19, *x)).skip(2).zip(b.iter());
    if let Some((x, y)) = z.next() {
        show(19, (x + y) as i64);
    }
    hint(z.size_hint());

    case(20); // `enumerate`, `copied`, `cloned` and `fuse` keep random access.
    for (i, x) in a.iter().map(|x| see(20, *x)).enumerate().zip(0..2) {
        show(20, (i.0 as i32 * 100 + i.1 + x) as i64);
    }
    let noisy = [Noisy(1), Noisy(2), Noisy(3)];
    for (n, x) in noisy.iter().cloned().zip(0..2) {
        show(20, (n.0 + x) as i64);
    }
    let mut z = noisy.iter().cloned().zip(0..2);
    while let Some((n, _)) = z.next_back() {
        show(20, n.0 as i64);
    }
    let s: i32 = a.iter().copied().map(|x| see(201, x)).fuse().zip(0..2).map(|(x, _)| x).sum();
    show(20, s as i64);

    case(21); // Nested zips.
    let s: i32 = a
        .iter()
        .map(|x| see(21, *x))
        .zip(b.iter().map(|x| see(210, *x)))
        .zip(0..2)
        .map(|((x, y), z)| x + y + z)
        .sum();
    show(21, s as i64);
    let s: i32 = a
        .iter()
        .rev()
        .map(|x| see(211, *x))
        .zip(b.iter().map(|x| see(212, *x)))
        .zip(0..2)
        .map(|((x, y), z)| x + y + z)
        .sum();
    show(21, s as i64);

    case(22); // `rev` of a zip, and `rev` twice.
    let s: i32 = a.iter().map(|x| see(22, *x)).zip(b.iter()).rev().map(|(x, y)| x * y).sum();
    show(22, s as i64);
    let s: i32 = a.iter().rev().map(|x| see(220, *x)).zip(0..2).rev().rev().map(|(x, _)| x).sum();
    show(22, s as i64);

    case(23); // `eq` of iterators that know their lengths.
    show(23, a.iter().map(|x| see(23, *x)).eq(b.iter().map(|x| see(230, *x))) as i64);
    show(23, a.iter().map(|x| see(231, *x)).eq(a.iter().rev().copied()) as i64);
    show(23, a.iter().filter(|x| see(232, **x) > 0).eq(b.iter()) as i64);
    show(23, (0..3).map(|x| see(233, x)).eq([0, 1, 2]) as i64);

    case(24); // Other ranges: wide integers, inclusive, characters.
    let n = (0u64..5).map(|x| see(24, x as i32)).zip(0..2).count();
    show(24, n as i64);
    let n = (0i128..5).map(|x| see(240, x as i32)).zip(0..2).count();
    show(24, n as i64);
    for (x, _) in (0i128..5).map(|x| see(241, x as i32)).zip(0..2) {
        show(24, x as i64);
    }
    let n = (0..=4).map(|x| see(242, x)).zip(0..2).count();
    show(24, n as i64);
    let n = ('a'..'e').map(|c| see(243, c as i32)).zip(0..2).count();
    show(24, n as i64);
    let n = (0..).map(|x| see(244, x)).zip(0..2).count();
    show(24, n as i64);

    case(25); // Bytes, chunks, windows and options.
    let s = "flux";
    let n = s.bytes().map(|c| see(25, c as i32)).zip(0..2).count();
    show(25, n as i64);
    for (c, _) in s.bytes().map(|c| see(250, c as i32)).zip(0..1) {
        show(25, c as i64);
    }
    let n = a.chunks(2).map(|c| see(251, c[0])).zip(0..2).count();
    show(25, n as i64);
    for (w, _) in a.windows(3).map(|w| see(252, w[2])).zip(0..1) {
        show(25, w as i64);
    }
    for (w, _) in a.chunks_exact(2).map(|w| see(253, w[1])).zip(0..1) {
        show(25, w as i64);
    }
    let n = Some(7).into_iter().map(|x| see(254, x)).zip(0..0).count();
    show(25, n as i64);

    case(26); // A mutable slice iterator, and a clone of a zip partway.
    let mut m = [1, 2, 3, 4];
    for (x, y) in m.iter_mut().zip(b.iter()) {
        *x += *y;
    }
    for x in m {
        show(26, x as i64);
    }
    let mut z = a.iter().map(|x| see(26, *x)).zip(b.iter());
    z.next();
    let w = z.clone();
    show(26, z.count() as i64);
    show(26, w.map(|(x, _)| x).sum::<i32>() as i64);

    case(27); // Flattened iterators fold each inner one.
    let s: i32 = [0, 1]
        .iter()
        .flat_map(|&i| a.iter().rev().map(move |x| see(27, x + i)).zip(0..2))
        .map(|(x, _)| x)
        .sum();
    show(27, s as i64);

    case(28); // Flattening arrays knows its length; flattening slices does not.
    let grid = [[1, 2], [3, 4], [5, 6]];
    let mut flat = grid.iter().flatten();
    hint(flat.size_hint());
    flat.next();
    hint(flat.size_hint());
    let n = grid.iter().flatten().map(|x| see(28, *x)).zip(0..3).count();
    show(28, n as i64);
    let n = grid.into_iter().flatten().map(|x| see(280, x)).zip(0..1).count();
    show(28, n as i64);
    let n = (0..3).flat_map(|i| [i, i * 10]).map(|x| see(281, x)).zip(0..3).count();
    show(28, n as i64);
    hint((0..3).flat_map(|i| [i, i * 10]).size_hint());
    let rows: [&[i32]; 2] = [&a[..2], &a[2..]];
    let n = rows.iter().flat_map(|r| r.iter()).map(|x| see(282, *x)).zip(0..2).count();
    show(28, n as i64);
    hint(rows.iter().flat_map(|r| r.iter()).size_hint());

    case(29); // `core::iter`'s sources know their lengths.
    let n = core::iter::repeat(4).map(|x| see(29, x)).zip(0..2).count();
    show(29, n as i64);
    let n = core::iter::repeat_n(5, 3).map(|x| see(290, x)).zip(0..2).count();
    show(29, n as i64);
    let n = core::iter::once(6).map(|x| see(291, x)).zip(0..0).count();
    show(29, n as i64);
    let n = core::iter::repeat_with(|| see(292, 7)).zip(0..2).count();
    show(29, n as i64);
    let n = core::iter::empty::<i32>().zip((0..3).map(|x| see(293, x))).count();
    show(29, n as i64);
    let mut k = 0;
    let n = core::iter::from_fn(|| {
        k += 1;
        Some(see(294, k))
    })
    .zip(0..2)
    .count();
    show(29, n as i64);

    case(30); // `partition`, `unzip` and `collect` fold too.
    let (small, big): (Tally, Tally) = a.iter().rev().map(|x| see(30, *x)).zip(0..3).partition(|p| p.0 < 4);
    show(30, (small.0 * 10 + big.0) as i64);
    let (left, right): (Tally, Tally) = a.iter().rev().map(|x| see(300, *x)).zip(0..2).unzip();
    show(30, (left.0 * 10 + right.0) as i64);
    let () = a.iter().rev().map(|x| see(301, *x)).zip(0..1).map(|_| ()).collect();

    case(31); // Trait objects are not trusted, whatever is behind them.
    let mut it = a.iter().map(|x| see(31, *x));
    let d: &mut dyn Iterator<Item = i32> = &mut it;
    show(31, d.zip(0..1).count() as i64);
    let mut it = a.iter().map(|x| see(310, *x));
    let d: &mut dyn Iterator<Item = i32> = &mut it;
    show(31, d.eq([1, 2].iter().copied()) as i64);
}

/// A collection that counts what it is given.
#[derive(Default)]
struct Tally(i32);

impl<T> Extend<T> for Tally {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for _ in iter {
            self.0 += 1;
        }
    }
}
