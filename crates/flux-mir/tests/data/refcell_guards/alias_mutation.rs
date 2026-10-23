use core::cell::RefCell;

fn main() {
    let cell = RefCell::new(vec![1]);
    let mut guard = cell.borrow_mut();
    let first = &mut *guard;
    let second = &mut *guard;
    first.push(2);
    second.push(3);
}
