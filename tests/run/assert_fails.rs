//! A failing `assert_eq!` panics, after what came before ran: with
//! `panic=abort` the program aborts (the message goes to stderr).

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn sum(values: &[i32]) -> i32 {
    let mut total = 0;
    for v in values {
        total += *v;
    }
    total
}

fn main() {
    let values = [1, 2, 3];
    assert!(sum(&values) > 0, "the sum is {}", sum(&values));
    unsafe { printf(c"%d\n".as_ptr(), sum(&values)) };
    debug_assert_ne!(values[0], values[1]);
    assert_eq!(sum(&values), 7, "values: {:?}", values);
    unsafe { printf(c"unreachable\n".as_ptr()) };
}
