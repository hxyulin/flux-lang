use core::cell::RefCell;

fn escape(cell: &RefCell<Vec<i32>>) -> &Vec<i32> {
    let guard = cell.borrow();
    &*guard
}
fn main() {}
