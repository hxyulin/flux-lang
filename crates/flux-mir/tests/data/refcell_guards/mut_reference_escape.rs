use core::cell::RefCell;

fn escape(cell: &RefCell<Vec<i32>>) -> &mut Vec<i32> {
    let mut guard = cell.borrow_mut();
    &mut *guard
}
fn main() {}
