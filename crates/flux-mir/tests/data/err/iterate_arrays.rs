// Iterating an array by value moves it, unless it is `Copy`; iterating
// it by mutable reference borrows it for the loop.

struct S(i32);

fn main() {
    let a = [S(1), S(2)];
    for s in a {
        let _ = s;
    }
    let n = a[0].0;
    let mut b = [1, 2, 3];
    for x in &mut b {
        *x += b[0];
    }
    let mut c = [1, 2];
    let mut iter = c.iter_mut();
    c[1] = 5;
    let _ = iter.next();
}
