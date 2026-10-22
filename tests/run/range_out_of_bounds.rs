//! Slicing with a range that is not within the slice panics, after what
//! came before ran: here a start after the end ("slice index starts at
//! 6 but ends at 4"), once an empty range at the end has worked.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn total(values: &[i32], lo: usize, hi: usize) -> i32 {
    let mut sum = 0;
    for x in &values[lo..hi] {
        sum += *x;
    }
    sum
}

fn main() {
    let values = [10, 20, 30, 40];
    let mut lo = 0;
    while lo < 8 {
        unsafe { printf(c"%d\n".as_ptr(), total(&values, lo, 4)) };
        lo += 2;
    }
}
