use std::cell::Cell;
use std::cmp::Ordering;
use std::ops::Bound::{Excluded, Included, Unbounded};

const PARTS: (&[u8], &[u8]) = [2, 3, 5, 7].split_at(2);
const CHECKED: Option<(&[u8], &[u8])> = [11, 13].split_at_checked(3);
static DATA: [u8; 4] = [17, 19, 23, 29];
const STATIC_PARTS: (&[u8], &[u8]) = DATA.split_at(1);
const fn divide(values: &[u8]) -> (&[u8], &[u8]) {
    values.split_at(1)
}
const INDIRECT: (&[u8], &[u8]) = divide(&[31, 37, 41]);

#[derive(Debug)]
struct Item<'a> {
    n: i32,
    drops: &'a Cell<usize>,
    clones: &'a Cell<usize>,
}
impl Clone for Item<'_> {
    fn clone(&self) -> Self {
        self.clones.set(self.clones.get() + 1);
        Self {
            n: self.n,
            drops: self.drops,
            clones: self.clones,
        }
    }
}
impl Drop for Item<'_> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

fn main() {
    assert_eq!(PARTS.0, &[2, 3]);
    assert_eq!(PARTS.1, &[5, 7]);
    assert!(CHECKED.is_none());
    assert_eq!(INDIRECT.1, &[37, 41]);
    assert_eq!(STATIC_PARTS.1.as_ptr(), unsafe { DATA.as_ptr().add(1) });
    let values = [1, 2, 3, 4];
    assert_eq!(values.split_at_checked(0).unwrap().0, &[]);
    assert_eq!(values.split_at_checked(4).unwrap().1, &[]);
    assert_eq!(values.strip_prefix(&[1, 2]), Some(&[3, 4][..]));
    assert!(values.strip_prefix(&[1, 3]).is_none());
    assert_eq!(values.strip_prefix(&[]), Some(&values[..]));
    let range = values.as_ptr_range();
    assert_eq!(range.start, values.as_ptr());
    assert_eq!(range.end, unsafe { values.as_ptr().add(4) });
    assert_eq!(&values[(Included(1), Excluded(3))], &[2, 3]);
    assert_eq!(&values[(Excluded(0), Included(2))], &[2, 3]);
    assert_eq!(&values[(Unbounded, Unbounded)], &values);
    assert!(values.get((Excluded(usize::MAX), Unbounded)).is_none());
    assert!(values.get((Unbounded, Included(usize::MAX))).is_none());
    assert!(values.get((Included(3), Excluded(2))).is_none());
    let split: Vec<_> = [0, 1, 0, 2, 0].split(|n| *n == 0).collect();
    assert_eq!(split, vec![&[][..], &[1][..], &[2][..], &[][..]]);
    let mut split = [0, 1, 0, 2, 0].split(|n| *n == 0);
    assert_eq!(split.next(), Some(&[][..]));
    assert_eq!(split.next_back(), Some(&[][..]));
    assert_eq!(split.next_back(), Some(&[2][..]));
    assert_eq!(split.next(), Some(&[1][..]));
    assert_eq!(split.next(), None);
    assert_eq!(split.next_back(), None);

    let mut mutable = [2, 3, 5, 7];
    let mut iter = mutable.iter_mut();
    let first = iter.next().unwrap();
    *first = 11;
    assert_eq!(iter.as_slice(), &[3, 5, 7]);
    let last = iter.next_back().unwrap();
    *last = 13;
    assert_eq!(iter.as_slice(), &[3, 5]);
    let remaining = iter.into_slice();
    remaining[0] = 17;
    *first += 1;
    *last += 0;
    assert_eq!(mutable, [12, 17, 5, 13]);
    let (left, right) = mutable.split_at_mut_checked(2).unwrap();
    left[0] = 19;
    right[0] = 23;
    assert_eq!(mutable, [19, 17, 23, 13]);
    assert!(mutable.split_at_mut_checked(5).is_none());
    let mut shared = values.iter();
    shared.next();
    assert_eq!(
        format!("{:?}", shared.enumerate()),
        "Enumerate { iter: Iter([2, 3, 4]), count: 0 }"
    );
    assert_eq!(
        format!("{:?}", mutable.iter_mut()),
        "IterMut([19, 17, 23, 13])"
    );
    assert_eq!([1, 2].into_iter().cmp([1, 3]), Ordering::Less);
    assert_eq!([1, 2].into_iter().cmp([1]), Ordering::Greater);
    assert_eq!(
        [1, 2].iter().partial_cmp([1, 2].iter()),
        Some(Ordering::Equal)
    );
    assert_eq!([f32::NAN].into_iter().partial_cmp([1.0]), None);
    assert!(![1.0, f32::NAN].is_sorted());
    assert!([1, 2, 2].is_sorted_by(|a, b| a <= b));
    assert!([3, 2, 1].is_sorted_by_key(|n| -n));

    let drops = Cell::new(0);
    let clones = Cell::new(0);
    {
        let mut owned: Vec<_> = (0..6)
            .map(|n| Item {
                n,
                drops: &drops,
                clones: &clones,
            })
            .collect();
        owned.retain_mut(|item| {
            item.n += 10;
            item.n % 2 == 0
        });
        assert_eq!(drops.get(), 3);
        let mut iter = owned.into_iter();
        drop(iter.next());
        drop(iter.next_back());
        iter.as_mut_slice()[0].n = 99;
        assert_eq!(
            format!("{:?}", iter).starts_with("IntoIter([Item { n: 99"),
            true
        );
        let copy = iter.clone();
        assert_eq!(clones.get(), 1);
        assert_eq!(copy.as_slice()[0].n, 99);
    }
    assert_eq!(drops.get(), 7);
    let mut keyed = vec![(2, 'a'), (1, 'b'), (2, 'c'), (0, 'd'), (1, 'e')];
    let calls = Cell::new(0);
    keyed.sort_by_cached_key(|item| {
        calls.set(calls.get() + 1);
        item.0
    });
    assert_eq!(calls.get(), 5);
    assert_eq!(
        keyed,
        vec![(0, 'd'), (1, 'b'), (1, 'e'), (2, 'a'), (2, 'c')]
    );
    let mut seed = 7u32;
    for len in 0..24 {
        for _ in 0..16 {
            let mut values = Vec::new();
            for index in 0..len {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                values.push((seed % 7, index));
            }
            let mut expected = values.clone();
            expected.sort_by_key(|item| item.0);
            values.sort_by_cached_key(|item| item.0);
            assert_eq!(values, expected);
        }
    }
    let mut zst = vec![(); 5];
    zst.retain_mut(|_| true);
    assert_eq!(zst.len(), 5);
    let mut zi = zst.iter_mut();
    let a = zi.next().unwrap();
    let b = zi.next_back().unwrap();
    assert_eq!(zi.as_slice().len(), 3);
    assert_eq!(zi.into_slice().len(), 3);
    *a = ();
    *b = ();
    let mut empty: [u32; 0] = [];
    let mut ei = empty.iter_mut();
    assert!(ei.next().is_none());
    assert!(ei.as_slice().is_empty());
    assert!(ei.into_slice().is_empty());
    println!("slice {} {} {}", drops.get(), clones.get(), calls.get());
}
