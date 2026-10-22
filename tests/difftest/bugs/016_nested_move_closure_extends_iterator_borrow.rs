//! Wrong rejection: a consumed iterator's shared capture is kept live
//! across assignment when its callback constructs a nested move closure.
//!
//! rustc accepts this and prints `true`. flux rejects the assignment to
//! `value.1` as borrowed, both with and without optimization. Removing
//! the nested closure makes flux accept the program.
//!
//! Found by difftest seed 109288 (also seen at 109495) with the default
//! weights after reenabling parenthesized patterns and bare macros.
//! Manually minimized; this is a borrow-checking issue, separate from
//! the repaired parser cases.

fn main() {
    let mut value = (0u8, false);
    value.1 = [1, 2, 3].into_iter().filter(|_| value.1).all(|_| {
        let _capture = move || value.1;
        true
    });
    println!("{}", value.1);
}
