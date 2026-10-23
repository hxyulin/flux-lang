use core::cell::RefCell;

fn main() {
    let cell = RefCell::new(vec![1]);
    let mut guard = cell.borrow();
    guard.push(2);
}
