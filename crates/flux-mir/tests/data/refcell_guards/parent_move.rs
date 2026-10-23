use core::cell::RefCell;

fn main() {
    let cell = RefCell::new(vec![1]);
    let guard = cell.borrow_mut();
    drop(cell);
    let _ = guard.len();
}
