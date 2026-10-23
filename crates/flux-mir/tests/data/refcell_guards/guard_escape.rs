use core::cell::RefCell;

fn main() {
    let guard;
    {
        let cell = RefCell::new(vec![1]);
        guard = cell.borrow();
    }
    let _ = guard.len();
}
