//! Conflicting dynamic borrows must panic while a real guard remains alive.
use core::cell::RefCell;

fn main() {
    let cell = RefCell::new(vec![1]);
    let mut guard = cell.borrow_mut();
    guard.push(2);
    let _conflict = cell.borrow_mut();
}
