//! Overloaded derefs borrow their base, while indexed captures also
//! evaluate their index and Copy uses keep non-move closures reusable.
extern crate alloc;
use alloc::boxed::Box;
use core::ops::{Deref, DerefMut};

struct Wrap<T> { value: T }
impl<T> Deref for Wrap<T> {
    type Target = T;
    fn deref(&self) -> &T { &self.value }
}
impl<T> DerefMut for Wrap<T> {
    fn deref_mut(&mut self) -> &mut T { &mut self.value }
}
fn twice<F: Fn() -> i32>(f: F) { println!("{} {}", f(), f()); }
fn sum(xs: &[i32; 2]) -> i32 { xs.iter().sum() }
fn main() {
    let mut w = Wrap { value: [3, 5] };
    let i = 1usize;
    twice(|| w[i]);
    twice(|| sum(&w));
    let wild = || { let _ = w[i]; };
    wild();
    wild();
    let mut update = || {
        let ref mut n = w[i];
        *n += 2;
        *n
    };
    println!("{} {}", update(), update());
    let mut matched = || match w[i] {
        ref mut n => { *n += 3; *n }
    };
    println!("{} {}", matched(), matched());
    let mut conditional = || {
        if let ref mut n = w[i] { *n += 4; }
        w[i]
    };
    println!("{} {}", conditional(), conditional());
    let mut coerced = || {
        let r: &mut [i32; 2] = &mut w;
        r[0] += 1;
        r[0]
    };
    println!("{} {}", coerced(), coerced());
    let copied = Wrap { value: 12i32 };
    twice(|| (*copied).abs());
    let nested = Wrap { value: Wrap { value: [7, 11] } };
    twice(|| nested[i]);
    twice(|| sum(&nested));

    let mut boxed = Box::new([13, 17]);
    twice(|| boxed[i]);
    let mut change = || {
        let ref mut n = boxed[i];
        *n += 1;
        *n
    };
    println!("{} {}", change(), change());
    twice(|| sum(&*boxed));
    let owned = Box::new((Box::new(19), Box::new(23)));
    let take = || *owned;
    let (a, b) = take();
    println!("{} {}", *a, *b);
}
