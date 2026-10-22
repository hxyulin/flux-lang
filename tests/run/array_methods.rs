//! Slices' methods called on arrays, references to arrays and mutable
//! references to them: the receiver is unsized to a slice.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn total(values: &[i64; 4]) -> i64 {
    let mut sum = 0;
    for x in values.iter() {
        sum += *x;
    }
    sum
}

fn last_or(values: &mut [i64; 3], default: i64) -> i64 {
    match values.last() {
        Some(x) => *x,
        None => default,
    }
}

fn main() {
    let a = [3i64, 1, 4, 1];
    let mut b = [5i64, 9, 2];
    let grid = [[1i64, 2], [3, 4], [5, 6]];
    let first = *a.first().unwrap();
    let missing = a.get(7).is_none();
    let rows: i64 = grid.iter().map(|row| row.iter().fold(0, |acc, x| acc + *x)).sum();
    let largest = *grid[2].iter().max().unwrap();
    let back = *a.iter().rev().next().unwrap();
    unsafe {
        printf(
            c"%lld %lld %d %lld %lld %lld %lld\n".as_ptr(),
            total(&a),
            first,
            missing as i32,
            last_or(&mut b, -1),
            rows,
            largest,
            back,
        );
    }
}
