//! Functions and adapters taking `U: IntoIterator` (`zip`, `chain`,
//! `eq`, `flat_map`, `flatten` and the program's own) with every kind
//! of `IntoIterator`: arrays by value, `&[T; N]`, `&mut [T; N]`, `&[T]`,
//! `&mut [T]`, `Option`, `&Option`, ranges and iterators. The signature
//! names `U::IntoIter`, which must be each one's own iterator.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

/// Sums what `items` gives, through its `IntoIter`.
fn total<U: IntoIterator<Item = i64>>(items: U) -> i64 {
    let mut it: U::IntoIter = items.into_iter();
    let mut sum = 0;
    while let Some(x) = it.next() {
        sum = sum * 10 + x;
    }
    sum
}

/// Counts what `items` gives, by reference or not.
fn count<U: IntoIterator>(items: U) -> i64 {
    let mut n = 0;
    for _ in items {
        n += 1;
    }
    n
}

/// Keeps the iterator in a struct, as `Zip` and `Chain` do.
struct Held<I> {
    iter: I,
}

fn hold<U: IntoIterator>(items: U) -> Held<U::IntoIter> {
    Held {
        iter: items.into_iter(),
    }
}

fn main() {
    let a = [1i64, 2, 3];
    let mut m = [4i64, 5, 6];
    let s: &[i64] = &[7, 8, 9];

    show(1, total(a));
    show(2, total(a.iter().copied()));
    show(3, total(s.iter().copied()));
    show(4, total(Some(5)));
    show(5, total(1..4));
    show(6, count(&a));
    show(7, count(&mut m));
    show(8, count(s));
    show(9, count(&Some(1)));
    show(10, count(None::<i32>));

    let z: i64 = a.iter().zip(s).map(|(x, y)| x * y).sum();
    show(11, z);
    let z: i64 = a.iter().zip(a).map(|(x, y)| x * 100 + y).sum();
    show(12, z);
    let z: i64 = a.iter().zip(&m).map(|(x, y)| x * y).sum();
    show(13, z);
    let n = s.iter().zip(&mut m).map(|(x, y)| {
        *y += *x;
        *y
    });
    show(14, n.sum());
    show(15, m[2]);
    let z: i64 = a.iter().zip(Some(10)).map(|(x, y)| x * y).sum();
    show(16, z);
    let z = (0..5).zip(2..).map(|(x, y)| x * y).sum::<i64>();
    show(17, z);
    let c: i64 = a.iter().chain(s).sum();
    show(18, c);
    let c: i64 = a.into_iter().chain([10, 20]).sum();
    show(19, c);
    let c: i64 = a.iter().copied().chain(Some(100)).chain(None).sum();
    show(20, c);
    show(21, a.iter().eq(&[1, 2, 3]) as i64);
    show(22, a.iter().copied().eq(a) as i64);
    show(23, a.iter().eq(s) as i64);
    let f: i64 = (1..3).flat_map(|n| [n, n * 10]).sum();
    show(24, f);
    let f: i64 = [Some(1), None, Some(3)].into_iter().flatten().sum();
    show(25, f);
    let f: i64 = [[1i64, 2], [3, 4]].iter().flatten().sum();
    show(26, f);
    let mut h = hold(a);
    h.iter.next();
    show(27, h.iter.next().unwrap());
    let mut h = hold(s);
    show(28, *h.iter.next().unwrap());
    let mut h = hold(Some(9));
    show(29, h.iter.next().unwrap());
    let mut h = hold(&mut m);
    *h.iter.next().unwrap() = 0;
    show(30, m[0]);
    let z: usize = a.iter().zip(s).len();
    show(31, z as i64);
}
