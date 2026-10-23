//! Conflicting dynamic borrows must panic while a real guard remains alive.
use core::cell::{Ref, RefCell};

fn main() {
    let cell = RefCell::new(vec![1]);
    let guard = cell.borrow();
    let _cloned = Ref::clone(&guard);
    drop(guard);
    let _conflict = cell.borrow_mut();
}
