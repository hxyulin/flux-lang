use std::cell::{Cell, Ref, RefCell, RefMut};
fn main() {
    let cell = RefCell::new([1, 2, 3, 4]);
    let (left, right) = Ref::map_split(cell.borrow(), |v| v.split_at(2));
    assert_eq!(&*left, &[1, 2]);
    drop(left);
    assert!(cell.try_borrow_mut().is_err());
    drop(right);
    let (mut left, right) = RefMut::map_split(cell.borrow_mut(), |v| v.split_at_mut(2));
    left[0] = 7;
    drop(left);
    assert!(cell.try_borrow().is_err());
    assert!(cell.try_borrow_mut().is_err());
    drop(right);
    assert_eq!(*cell.borrow(), [7, 2, 3, 4]);
    let mut values = [1, 2];
    let cells: &Cell<[i32]> = Cell::from_mut(&mut values[..]);
    assert_eq!(
        cells.as_ptr() as *mut i32 as usize,
        values.as_mut_ptr() as usize
    );
    println!("split loans and unsized cell passed");
}
