//! `core::cell`: `Cell` (`get`, `set`, `replace`, `take`, `swap`,
//! `update`, through shared references, as RAII guards use it) and
//! `RefCell`, whose loans are counted, and whose `borrow_mut` while
//! borrowed panics, as rustc's does; and inline constants (`const { ..
//! }`), as a repeat expression's operand of a type that is not `Copy`
//! too.

use core::cell::{Cell, RefCell};

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

struct Guard<'a> {
    count: &'a Cell<i32>,
    id: i32,
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.count.set(self.count.get() * 10 + self.id);
    }
}

fn bump(c: &Cell<u32>) -> u32 {
    c.set(c.get() + 1);
    c.get()
}

fn doubled<const N: usize>() -> usize {
    const { N * 2 }
}

fn main() {
    let shared = [const { Cell::new(0) }; 4];
    {
        let _a = Guard { count: &shared[1], id: 4 };
        let _b = Guard { count: &shared[1], id: 5 };
    }
    show(30, shared[1].get() as i64 + shared[0].get() as i64);
    let small: u8 = const { 200 + 5 };
    show(31, small as i64 + doubled::<21>() as i64);
    let log = Cell::new(0);
    {
        let _a = Guard { count: &log, id: 1 };
        let _b = Guard { count: &log, id: 2 };
        let _c = Guard { count: &log, id: 3 };
    }
    show(1, log.get() as i64);
    let c = Cell::new(5u32);
    show(2, bump(&c) as i64 + bump(&c) as i64);
    show(3, c.replace(40) as i64);
    show(4, c.take() as i64);
    show(5, c.get() as i64);
    c.update(|x| x + 7);
    show(6, c.into_inner() as i64);
    let a = Cell::new(1);
    let b = Cell::new(2);
    a.swap(&b);
    show(7, a.get() * 10 + b.get());
    let mut m = Cell::new(3);
    *m.get_mut() += 1;
    show(8, m.get() as i64);
    show(9, (Cell::new(3) == Cell::new(3)) as i64);
    let cl = a.clone();
    a.set(9);
    show(10, cl.get() * 10 + a.get());
    let cells = [Cell::new(1), Cell::new(2), Cell::new(3)];
    for cell in &cells {
        cell.set(cell.get() * 2);
    }
    show(11, cells.iter().map(|c| c.get()).sum::<i32>() as i64);
    let r = RefCell::new(10);
    {
        let x = r.borrow();
        let y = r.borrow();
        show(12, r.try_borrow_mut().is_err() as i64);
        let z = core::cell::Ref::clone(&x);
        drop(z);
        drop(y);
        show(13, r.try_borrow().is_ok() as i64);
        drop(x);
    }
    show(14, r.try_borrow_mut().is_ok() as i64);
    {
        let _m = r.borrow_mut();
        show(15, r.try_borrow().is_err() as i64);
        show(16, r.try_borrow_mut().is_err() as i64);
    }
    show(17, r.replace(20) as i64);
    show(18, r.replace_with(|v| *v + 1) as i64);
    show(19, r.take() as i64);
    let s = RefCell::new(5);
    r.swap(&s);
    show(20, r.into_inner() as i64 * 100 + s.into_inner() as i64);
    let p = RefCell::new(3);
    let _held = p.borrow();
    let _boom = p.borrow_mut();
    show(21, 0);
}
