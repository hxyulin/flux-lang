// A closure calling a method through a captured `&mut` borrows what it
// points to, and one calling `len` borrows the array: neither moves it,
// so the errors are of borrows, as in rustc.

struct S {
    v: i32,
}

impl S {
    fn get(&self) -> i32 {
        self.v
    }
}

fn main() {
    let mut s = S { v: 1 };
    let m = &mut s;
    let f = || m.get();
    m.v = 2;
    f();
    let a = [S { v: 1 }];
    let g = || a.len();
    let b = a;
    g();
    let _ = b;
}

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
fn copy_deref() {
    let mut w = Wrap { value: 1i32 };
    let read = || (*w).abs();
    *w = 2;
    println!("{}", read());
}
fn wildcard_index() {
    let mut w = Wrap { value: [1, 2] };
    let i = 0usize;
    let read = || { let _ = w[i]; };
    w[0] = 3;
    read();
}
fn mutable_index() {
    let mut w = Wrap { value: [1, 2] };
    let mut update = || { let ref mut n = w[0]; *n += 1; };
    println!("{}", w[0]);
    update();
}
fn boxed_pointee() {
    let b = Box::new(1);
    let r = &*b;
    drop(b);
    println!("{}", r);
}
fn boxed_field() {
    let b = Box::new((1, 2));
    let r = &b.0;
    drop(b);
    println!("{}", r);
}
