use core::cell::RefCell;

fn main() {
    let cell = RefCell::new(vec![1]);
    let mut guard = cell.borrow_mut();
    let shared = &*guard;
    guard.push(2);
    let _ = shared.len();
}
