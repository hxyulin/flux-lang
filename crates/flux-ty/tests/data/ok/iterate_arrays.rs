// Arrays are iterable by value, through the compiler's `IntoIterator`
// for `[T; N]` (rustc's `core` writes it with const generics), and by
// reference, shared or mutable, as slices of them.

fn total<I: IntoIterator<Item = i32>>(items: I) -> i32 {
    let mut sum = 0;
    for x in items {
        sum += x;
    }
    sum
}

fn main() {
    let a = [1, 2, 3];
    for x in a {}
    for x in &a {}
    let mut b = [4, 5];
    for x in &mut b {
        *x += 1;
    }
    let s: &[i32] = &a;
    for x in s {}
    let mut iter = a.into_iter();
    let first = iter.next();
    let last = iter.next_back();
    let rest = iter.as_slice();
    let by_ref = (&a).into_iter();
    let by_mut = (&mut b).into_iter();
    let mut m = [7u8, 8];
    for x in m.iter_mut() {
        *x = 0;
    }
    let sum = total([1, 2]) + total(a);
    let grid = [[1u8; 2]; 3];
    for row in grid {
        for cell in row {}
    }
}
