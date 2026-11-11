use std::cell::{Ref, RefCell};
fn main() {
    let cell = RefCell::new((17u32, [23u32, 31]));
    let loan = cell.borrow();
    let clone = Ref::clone(&loan);
    let projected: Ref<'_, [u32]> = Ref::map(loan, |pair| &pair.1[..]);
    assert_eq!(projected.len(), 2);
    assert_eq!(projected[0], 23);
    assert!(cell.try_borrow_mut().is_err());
    drop(clone);
    assert!(cell.try_borrow_mut().is_err());
    drop(projected);
    cell.borrow_mut().0 += 1;
    let mapped = Ref::map(cell.borrow(), |pair| &pair.0);
    assert_eq!(*mapped, 18);
    let cloned = Ref::clone(&mapped);
    drop(mapped);
    assert!(cell.try_borrow_mut().is_err());
    println!("{}", *cloned);
    drop(cloned);
    assert!(cell.try_borrow_mut().is_ok());
}
