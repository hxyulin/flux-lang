//! `core`'s macros that need no formatting: `assert!` and friends,
//! `matches!`, `unreachable!`, `line!`, `stringify!`; a macro defined in
//! a function sees the locals before it, as rustc's hygiene has it.

unsafe extern "C" {
    fn printf(fmt: *const i8, ...) -> i32;
}

macro_rules! sq {
    ($e:expr) => {
        $e * $e
    };
}

macro_rules! sum {
    () => { 0 };
    ($x:expr $(, $rest:expr)*) => { $x + sum!($($rest),*) };
}

macro_rules! make_fn {
    ($name:ident, $v:expr) => {
        fn $name() -> i32 {
            $v
        }
    };
}

make_fn!(seven, 7);

#[derive(Debug, PartialEq)]
enum Cell {
    Dead,
    Alive(u8),
}

fn check(x: i32) -> i32 {
    let a = 1;
    macro_rules! add_a {
        ($e:expr) => {
            $e + a
        };
    }
    let a = 100;
    add_a!(x) + a
}

fn main() {
    let x = sq!(2 + 1);
    let s = sum!(1, 2, 3, 4);
    let c = Cell::Alive(3);
    let alive = matches!(c, Cell::Alive(n) if n > 2);
    assert!(alive);
    assert_eq!(s, 10);
    assert_ne!(x, 10, "x was {}", x);
    debug_assert!(x == 9);
    let left_val = 5;
    assert_eq!(left_val, 5);
    unsafe {
        printf(c"%d %d %d %d %d\n".as_ptr(), x, s, alive as i32, seven(), check(1));
    }
    if x > 100 {
        unreachable!("x is {}", x);
    }
    assert!(Cell::Dead == Cell::Dead, "cells");
    let l = line!();
    let _ = l;
    let name = stringify!(a + b);
    let _ = name;
    if s == 11 {
        panic!("bad sum");
    }
    if s == 12 {
        todo!()
    }
}
