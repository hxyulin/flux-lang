//! A panic in a parenthesized expression reports the opening
//! parenthesis as its location, as rustc's does: rustc gives the
//! expression inside the parentheses their span.

use core::hint::black_box as bb;

fn main() {
    let a = [1, 2, 3];
    // In range: no panic, but the same spans.
    println!("{}", (a[bb(1)]) + (bb(10) / bb(3)));
    let x = (bb(i32::MIN) / bb(-1));
    println!("{x}");
}
