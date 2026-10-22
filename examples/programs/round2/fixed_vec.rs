//! A fixed-capacity vector `ArrayVec<T, N>` over `[T; N]` (no heap), the
//! way a `heapless`-style crate writes it: `push`/`pop`/`insert`/`remove`,
//! `Deref` to a slice (so every slice method works), `Index`/`IndexMut`,
//! `IntoIterator` by value, by `&` and by `&mut`, `FromIterator` (so
//! `collect` works), `Extend`, `retain`, `dedup` and a capacity error with
//! `?`. Exercised on integers and on a small `Copy` struct.

use core::ops::{Deref, DerefMut, Index, IndexMut};

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
    fn exit(code: i32) -> !;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapacityError<T>(pub T);

pub struct ArrayVec<T, const N: usize> {
    items: [T; N],
    len: usize,
}

impl<T: Copy + Default, const N: usize> ArrayVec<T, N> {
    pub fn new() -> Self {
        ArrayVec { items: [T::default(); N], len: 0 }
    }

    pub const fn capacity(&self) -> usize {
        N
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_full(&self) -> bool {
        self.len == N
    }

    pub fn try_push(&mut self, value: T) -> Result<(), CapacityError<T>> {
        if self.len == N {
            return Err(CapacityError(value));
        }
        self.items[self.len] = value;
        self.len += 1;
        Ok(())
    }

    pub fn push(&mut self, value: T) {
        self.try_push(value).ok().expect("ArrayVec is full");
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            None
        } else {
            self.len -= 1;
            Some(self.items[self.len])
        }
    }

    pub fn insert(&mut self, index: usize, value: T) -> Result<(), CapacityError<T>> {
        assert!(index <= self.len, "insertion index out of bounds");
        if self.is_full() {
            return Err(CapacityError(value));
        }
        let mut i = self.len;
        while i > index {
            self.items[i] = self.items[i - 1];
            i -= 1;
        }
        self.items[index] = value;
        self.len += 1;
        Ok(())
    }

    pub fn remove(&mut self, index: usize) -> T {
        assert!(index < self.len, "removal index out of bounds");
        let value = self.items[index];
        self.items.copy_within(index + 1..self.len, index);
        self.len -= 1;
        value
    }

    pub fn retain<F: FnMut(&T) -> bool>(&mut self, mut keep: F) {
        let mut write = 0;
        for read in 0..self.len {
            let item = self.items[read];
            if keep(&item) {
                self.items[write] = item;
                write += 1;
            }
        }
        self.len = write;
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }
}

impl<T: Copy + Default + PartialEq, const N: usize> ArrayVec<T, N> {
    pub fn dedup(&mut self) {
        if self.len < 2 {
            return;
        }
        let mut write = 1;
        for read in 1..self.len {
            if self.items[read] != self.items[write - 1] {
                self.items[write] = self.items[read];
                write += 1;
            }
        }
        self.len = write;
    }
}

impl<T, const N: usize> Deref for ArrayVec<T, N> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.items[..self.len]
    }
}

impl<T, const N: usize> DerefMut for ArrayVec<T, N> {
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.items[..self.len]
    }
}

impl<T, const N: usize> Index<usize> for ArrayVec<T, N> {
    type Output = T;
    fn index(&self, i: usize) -> &T {
        &self.items[..self.len][i]
    }
}

impl<T, const N: usize> IndexMut<usize> for ArrayVec<T, N> {
    fn index_mut(&mut self, i: usize) -> &mut T {
        &mut self.items[..self.len][i]
    }
}

impl<T: Copy + Default, const N: usize> Default for ArrayVec<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy + Default, const N: usize> FromIterator<T> for ArrayVec<T, N> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut v = ArrayVec::new();
        for x in iter {
            if v.try_push(x).is_err() {
                break;
            }
        }
        v
    }
}

impl<T: Copy + Default, const N: usize> Extend<T> for ArrayVec<T, N> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for x in iter {
            self.push(x);
        }
    }
}

/// The by-value iterator: copies elements out from the front.
pub struct IntoIter<T, const N: usize> {
    vec: ArrayVec<T, N>,
    next: usize,
}

impl<T: Copy, const N: usize> Iterator for IntoIter<T, N> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        if self.next < self.vec.len {
            let x = self.vec.items[self.next];
            self.next += 1;
            Some(x)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.vec.len - self.next;
        (n, Some(n))
    }
}

impl<T: Copy, const N: usize> IntoIterator for ArrayVec<T, N> {
    type Item = T;
    type IntoIter = IntoIter<T, N>;
    fn into_iter(self) -> IntoIter<T, N> {
        IntoIter { vec: self, next: 0 }
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a ArrayVec<T, N> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a mut ArrayVec<T, N> {
    type Item = &'a mut T;
    type IntoIter = core::slice::IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Point {
    x: i32,
    y: i32,
}

fn print_ints(label: &str, xs: &[i32]) {
    unsafe {
        printf(c"%.*s [".as_ptr(), label.len() as i32, label.as_ptr());
        for (i, x) in xs.iter().enumerate() {
            if i > 0 {
                printf(c", ".as_ptr());
            }
            printf(c"%d".as_ptr(), *x);
        }
        printf(c"] (len %zu)\n".as_ptr(), xs.len());
    }
}

fn fill_squares<const N: usize>(v: &mut ArrayVec<i32, N>) -> Result<usize, CapacityError<i32>> {
    let mut pushed = 0;
    for i in 1.. {
        v.try_push(i * i)?;
        pushed += 1;
    }
    Ok(pushed)
}

fn main() {
    let mut v: ArrayVec<i32, 16> = ArrayVec::new();
    for i in 0..10 {
        v.push(i * 3 % 7);
    }
    print_ints("pushed", &v);

    // Slice methods through Deref.
    v.sort();
    print_ints("sorted", &v);
    v.dedup();
    print_ints("dedup", &v);
    let pos = v.binary_search(&4);
    unsafe { printf(c"binary_search(4) = %d\n".as_ptr(), pos.unwrap_or(99) as i32) };
    v.reverse();
    print_ints("reversed", &v);
    unsafe {
        printf(
            c"first %d last %d contains(5) %d\n".as_ptr(),
            *v.first().unwrap(),
            *v.last().unwrap(),
            v.contains(&5) as i32,
        )
    };

    // IndexMut and iter_mut through &mut.
    v[0] = 100;
    for x in &mut v {
        *x += 1;
    }
    print_ints("bumped", &v);

    v.insert(2, -5).unwrap();
    let removed = v.remove(0);
    unsafe { printf(c"removed %d\n".as_ptr(), removed) };
    print_ints("insert/remove", &v);

    v.retain(|&x| x % 2 == 0);
    print_ints("evens", &v);

    // collect into an ArrayVec, then consume by value.
    let odds: ArrayVec<i32, 8> = (1..100).filter(|n| n % 2 == 1).map(|n| n * n).collect();
    print_ints("odd squares (cap 8)", &odds);
    let total: i32 = odds.into_iter().sum();
    unsafe { printf(c"sum of odd squares %d\n".as_ptr(), total) };

    // Extend and windows.
    let mut w: ArrayVec<i32, 12> = ArrayVec::default();
    w.extend([1, 4, 9, 16, 25, 36]);
    w.extend((7..=9).rev());
    let diffs: ArrayVec<i32, 12> = w.windows(2).map(|p| p[1] - p[0]).collect();
    print_ints("extended", &w);
    print_ints("diffs", &diffs);

    // Capacity errors propagate through `?`.
    let mut small: ArrayVec<i32, 5> = ArrayVec::new();
    match fill_squares(&mut small) {
        Ok(n) => unsafe { printf(c"filled %zu\n".as_ptr(), n); },
        Err(CapacityError(rejected)) => unsafe {
            printf(c"full after %zu, rejected %d\n".as_ptr(), small.len(), rejected);
        },
    }
    print_ints("small", &small);
    while let Some(x) = small.pop() {
        if x < 5 {
            break;
        }
    }
    print_ints("after pops", &small);

    // A struct element type, sorted by key and searched.
    let mut pts: ArrayVec<Point, 8> = ArrayVec::new();
    for (x, y) in [(3, 1), (-2, 4), (0, 0), (5, -5), (1, 1)] {
        pts.push(Point { x, y });
    }
    pts.sort_by_key(|p| p.x * p.x + p.y * p.y);
    for p in &pts {
        unsafe { printf(c"(%d,%d) ".as_ptr(), p.x, p.y) };
    }
    unsafe { printf(c"\n".as_ptr()) };
    let far = pts.iter().max_by_key(|p| p.x.abs() + p.y.abs()).copied().unwrap();
    let origin = pts.iter().position(|p| *p == Point::default());
    unsafe {
        printf(c"farthest (%d,%d), origin at %d, cap %zu\n".as_ptr(), far.x, far.y,
            origin.map_or(-1, |i| i as i32), pts.capacity())
    };
    pts.sort();
    assert!(pts.is_sorted());
    pts.clear();
    assert!(pts.is_empty());

    let checksum = w.iter().zip(diffs.iter()).fold(0i32, |acc, (a, b)| acc.wrapping_mul(31).wrapping_add(a * b));
    unsafe { exit(checksum & 0x7f) }
}
