// Slices' methods on arrays: the receiver is borrowed and unsized to a
// slice past its last dereference, as rustc's method probe does, whether
// it is an array, a reference to one or a mutable reference to one.

fn total(values: &[i32; 4]) -> i32 {
    let mut sum = 0;
    for x in values.iter() {
        sum += *x;
    }
    sum
}

fn last_of(values: &mut [u8; 2]) -> u8 {
    match values.last() {
        Some(x) => *x,
        None => 0,
    }
}

fn main() {
    let a = [3, 1, 4, 1];
    let first = a.first();
    let second = a.get(1);
    let count = a.iter().count();
    let mut b = [5u8, 6];
    let last = last_of(&mut b);
    let nested = [[1, 2], [3, 4]];
    let inner = nested[1].first();
    let rows = nested.iter().map(|row| row.len()).sum::<usize>();
    let _ = (first, second, count, last, inner, rows, total(&a));
}
