use core::cell::RefCell;

fn main() {
    let cell = RefCell::new(vec![1]);
    let mut guard = cell.borrow_mut();
    let shared = &guard;
    shared.push(2);
}
