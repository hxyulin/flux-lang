//! `Debug` and `Display` of `core`'s newer types, as rustc writes them:
//! `Reverse`, `ControlFlow`, `Bound`, `Cell`, `RefCell` (lent or not),
//! `Ref`, `RefMut`, the borrow errors, `TryFromIntError`,
//! `CharTryFromError`, `TryFromCharError`, `BuildHasherDefault`,
//! `iter::Empty` and `iter::Repeat`.

use core::cell::{Cell, RefCell};
use core::cmp::Reverse;
use core::hash::{BuildHasherDefault, Hasher};
use core::ops::{Bound, ControlFlow};

#[derive(Default)]
struct H;

impl Hasher for H {
    fn finish(&self) -> u64 {
        0
    }
    fn write(&mut self, _: &[u8]) {}
}

fn main() {
    println!("{:?} {:?}", Reverse(3), Reverse("x"));
    let c: ControlFlow<i32, u8> = ControlFlow::Break(4);
    let d: ControlFlow<i32> = ControlFlow::Continue(());
    println!("{:?} {:?} {:#?}", c, d, c);
    println!("{:?} {:?}", Bound::Included(3), Bound::<u8>::Unbounded);
    let cell = Cell::new(5);
    println!("{:?} {:#?}", cell, cell);
    let r = RefCell::new(7);
    println!("{:?}", r);
    {
        let _m = r.borrow_mut();
        println!("{:?}", r);
        println!("{:?} {}", r.try_borrow().err().unwrap(), r.try_borrow().err().unwrap());
        println!("{:?} {}", r.try_borrow_mut().err().unwrap(), r.try_borrow_mut().err().unwrap());
    }
    let b = r.borrow();
    println!("{:?}", b);
    drop(b);
    println!("{:?}", r.borrow_mut());
    let e = u8::try_from(300i32).unwrap_err();
    println!("{:?} {}", e, e);
    let ce = char::try_from(0xD800u32).unwrap_err();
    println!("{:?} {}", ce, ce);
    let te = u8::try_from('ā').unwrap_err();
    println!("{:?} {}", te, te);
    println!("{:?}", BuildHasherDefault::<H>::default());
    println!("{:?}", core::iter::empty::<u8>());
    println!("{:?}", core::iter::repeat(1));
}
