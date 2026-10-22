//! `Iterator`'s adapters and consumers beyond `iterators.rs`: `copied`,
//! `cloned`, sums and products of references, `size_hint` of every
//! adapter and range, `ExactSizeIterator::len`, `rposition`, `peekable`,
//! `scan`, `map_while`, `inspect`, `cycle`, `fuse`, `by_ref`,
//! `flat_map`, `flatten`, `reduce`, back ends of adapters, and the
//! `Option` methods `copied`, `cloned`, `as_ref`, `as_mut`, `take`,
//! `replace`, `insert`, `get_or_insert_with` and `core::mem::{swap,
//! replace}`.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

fn hint(label: i32, h: (usize, Option<usize>)) {
    let upper = match h.1 {
        Some(n) => n as i64,
        None => -1,
    };
    unsafe {
        printf(c"%d: hint %llu %lld\n".as_ptr(), label, h.0 as u64, upper);
    }
}

#[derive(Clone, PartialEq)]
struct Big {
    v: i32,
}

/// An iterator that is not fused: `None` every third call.
struct Flaky {
    n: i32,
}

impl Iterator for Flaky {
    type Item = i32;
    fn next(&mut self) -> Option<i32> {
        self.n += 1;
        if self.n % 3 == 0 { None } else { Some(self.n) }
    }
}

struct Loud(i32);

impl Drop for Loud {
    fn drop(&mut self) {
        show(900, self.0 as i64);
    }
}

struct Owner {
    _l: Loud,
    left: u32,
}

impl Iterator for Owner {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.left == 0 {
            None
        } else {
            self.left -= 1;
            Some(self.left)
        }
    }
}

fn sum_refs(s: &[u32]) -> u32 {
    s.iter().sum()
}

fn product_refs(s: &[i64]) -> i64 {
    s.iter().product()
}

fn main() {
    let a = [3u32, 1, 4, 1, 5, 9, 2, 6];
    let s: &[u32] = &a;

    // Option<&T>.
    show(1, s.first().copied().unwrap() as i64);
    show(2, s.get(7).cloned().unwrap_or(0) as i64);
    show(3, s.get(70).copied().is_none() as i64);
    let bigs = [Big { v: 7 }, Big { v: 8 }];
    let bs: &[Big] = &bigs;
    show(4, bs.last().cloned().unwrap().v as i64);

    // Sums and products of references.
    show(5, sum_refs(s) as i64);
    let ls = [2i64, -3, 5];
    show(6, product_refs(&ls));
    let t: u32 = s.iter().copied().filter(|&x| x > 2).sum();
    show(7, t as i64);
    let c: i32 = bs.iter().cloned().map(|b| b.v).sum();
    show(8, c as i64);
    show(9, s.iter().copied().max().unwrap() as i64);

    // size_hint and len.
    hint(10, s.iter().size_hint());
    hint(11, (0..10).size_hint());
    hint(12, (5..2).size_hint());
    hint(13, (0..=10u8).size_hint());
    hint(14, (0u64..).size_hint());
    hint(15, (i64::MIN..i64::MAX).size_hint());
    hint(16, (0u128..u128::MAX).size_hint());
    hint(17, (-5i128..=5).size_hint());
    hint(18, s.iter().filter(|x| **x > 3).size_hint());
    hint(19, s.iter().skip(3).size_hint());
    hint(20, s.iter().take(3).size_hint());
    hint(21, s.iter().take(30).size_hint());
    hint(22, s.iter().step_by(3).size_hint());
    hint(23, s.iter().chain(s.iter()).size_hint());
    hint(24, s.iter().zip(0..3).size_hint());
    hint(25, (0..).zip(s.iter()).size_hint());
    hint(26, s.iter().enumerate().rev().size_hint());
    hint(27, s.iter().map(|x| x * 2).take_while(|x| *x < 9).size_hint());
    hint(28, s.iter().skip_while(|x| **x < 9).size_hint());
    hint(29, (0..3).cycle().size_hint());
    hint(30, (0..0).cycle().size_hint());
    hint(31, (0..).filter(|x| x % 2 == 0).cycle().size_hint());
    hint(32, s.iter().flat_map(|&x| 0..x).size_hint());
    hint(33, s.iter().filter_map(|&x| if x > 3 { Some(x) } else { None }).size_hint());
    show(34, s.iter().len() as i64);
    show(35, (0..17u8).len() as i64);
    show(36, s.iter().skip(2).rev().len() as i64);
    let mut st = (0..20).step_by(6);
    show(37, st.len() as i64);
    st.next();
    hint(38, st.size_hint());
    let mut steps = 0..10;
    steps.next();
    steps.next_back();
    hint(39, steps.size_hint());
    let mut incl = 1..=3i16;
    incl.next();
    incl.next();
    incl.next();
    hint(40, incl.size_hint());

    // rposition, reduce, by_ref.
    show(41, s.iter().rposition(|&x| x == 1).unwrap() as i64);
    show(42, s.iter().rposition(|&x| x == 100).is_none() as i64);
    show(43, s.iter().copied().reduce(|a, b| a * 10 + b).unwrap() as i64);
    let mut it = s.iter();
    let firsts: u32 = it.by_ref().take(3).sum();
    show(44, firsts as i64);
    show(45, it.next().copied().unwrap() as i64);
    let mut total = 0;
    for x in &mut it {
        total += *x;
    }
    show(46, total as i64);

    // peekable.
    let mut p = s.iter().copied().peekable();
    show(47, *p.peek().unwrap() as i64);
    show(48, p.next().unwrap() as i64);
    hint(49, p.size_hint());
    p.peek();
    hint(50, p.size_hint());
    if let Some(x) = p.peek_mut() {
        *x = 100;
    }
    show(51, p.next().unwrap() as i64);
    show(52, p.next_if(|&x| x == 4).unwrap() as i64);
    show(53, p.next_if(|&x| x == 4).is_none() as i64);
    show(54, p.next_if_eq(&1).unwrap() as i64);
    show(55, p.next_back().unwrap() as i64);
    let mut words = 0;
    let runs: &[i32] = &[1, 1, 2, 3, 3, 3, 4];
    let mut q = runs.iter().peekable();
    while let Some(&x) = q.next() {
        while q.peek() == Some(&&x) {
            q.next();
        }
        words += 1;
    }
    show(56, words);
    let mut e = (0..0).peekable();
    show(57, e.peek().is_none() as i64);
    hint(58, e.size_hint());

    // scan, map_while, inspect.
    let running: i64 = s
        .iter()
        .scan(0u32, |acc, &x| {
            *acc += x;
            if *acc > 20 { None } else { Some(*acc as i64) }
        })
        .sum();
    show(59, running);
    let mut seen = 0;
    let n = s
        .iter()
        .map_while(|&x| if x < 5 { Some(x) } else { None })
        .inspect(|x| seen += *x)
        .count();
    show(60, n as i64);
    show(61, seen as i64);
    let mut back = 0;
    let last = s.iter().inspect(|x| back = back * 10 + **x).rev().take(2).last();
    show(62, *last.unwrap() as i64);
    show(63, back as i64);

    // cycle, fuse.
    let cyc: u32 = s.iter().take(3).cycle().take(10).sum();
    show(64, cyc as i64);
    let mut flaky = Flaky { n: 0 };
    show(65, flaky.next().unwrap() as i64);
    show(66, flaky.next().unwrap() as i64);
    show(67, flaky.next().is_none() as i64);
    show(68, flaky.next().unwrap() as i64);
    let mut fused = Flaky { n: 0 }.fuse();
    fused.next();
    fused.next();
    show(69, fused.next().is_none() as i64);
    show(70, fused.next().is_none() as i64);
    hint(71, fused.size_hint());
    {
        let mut owned = Owner { _l: Loud(1), left: 2 }.fuse();
        show(72, owned.next().unwrap() as i64);
        show(73, owned.next().unwrap() as i64);
        show(74, owned.next().is_none() as i64);
        show(75, 75);
    }

    // flat_map, flatten.
    let fm: u32 = s.iter().take(4).flat_map(|&x| 0..x).sum();
    show(76, fm as i64);
    let mut both = (1..4).flat_map(|x| 0..x);
    show(77, both.next().unwrap() as i64);
    show(78, both.next_back().unwrap() as i64);
    show(79, both.count() as i64);
    let ranges = [1..3, 3..5, 5..7];
    let rs: &[core::ops::Range<i32>] = &ranges;
    let flat: i32 = rs.iter().cloned().flatten().sum();
    show(80, flat as i64);
    let opts = [Some(1), None, Some(3)];
    let os: &[Option<i32>] = &opts;
    let somes: i32 = os.iter().map(|o| *o).flat_map(|o| o.map(|v| 0..v)).flatten().sum();
    show(81, somes as i64);
    let mut fl = (0..3).map(|x| x..3).flatten();
    show(82, fl.next_back().unwrap() as i64);
    hint(83, fl.size_hint());
    show(84, fl.count() as i64);

    // Back ends.
    let mut en = s.iter().enumerate();
    let (i, x) = en.next_back().unwrap();
    show(85, i as i64 * 100 + *x as i64);
    let mut tk = s.iter().take(5);
    show(86, *tk.next_back().unwrap() as i64);
    let mut sk = s.iter().skip(5);
    show(87, *sk.next_back().unwrap() as i64);
    show(88, *sk.next_back().unwrap() as i64);
    let ch: u32 = s.iter().chain(s.iter().take(2)).rev().take(3).sum();
    show(89, ch as i64);
    let fmb = s.iter().filter_map(|&x| if x % 2 == 0 { Some(x) } else { None }).next_back();
    show(90, fmb.unwrap() as i64);
    show(91, s.iter().copied().rev().next().unwrap() as i64);

    // Option methods and mem.
    let mut o = Some(5);
    if let Some(v) = o.as_mut() {
        *v += 1;
    }
    show(92, *o.as_ref().unwrap() as i64);
    show(93, o.take().unwrap() as i64);
    show(94, o.is_none() as i64);
    show(95, o.replace(9).is_none() as i64);
    show(96, o.replace(10).unwrap() as i64);
    *o.insert(11) += 1;
    show(97, o.unwrap() as i64);
    let mut none: Option<i32> = None;
    *none.get_or_insert_with(|| 20) += 1;
    show(98, none.unwrap() as i64);
    show(99, *none.get_or_insert(50) as i64);
    let mut x = Big { v: 1 };
    let mut y = Big { v: 2 };
    core::mem::swap(&mut x, &mut y);
    show(100, (x.v * 10 + y.v) as i64);
    let old = core::mem::replace(&mut x, Big { v: 3 });
    show(101, (old.v * 10 + x.v) as i64);
    let mut l1 = Loud(5);
    let mut l2 = Loud(6);
    core::mem::swap(&mut l1, &mut l2);
    show(102, l1.0 as i64);
    let taken = o.take_if(|v| *v > 100);
    show(103, taken.is_none() as i64);
    let taken = o.take_if(|v| *v == 12);
    show(104, taken.unwrap() as i64);
}
