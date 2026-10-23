//! Dereference the real RefCell guards, preserving borrow counting and drops.
use core::cell::{Ref, RefCell};

fn shared(values: &Vec<i32>) -> i32 {
    values.iter().sum()
}

fn mutable(values: &mut Vec<i32>) {
    values.push(30);
}

fn main() {
    let cell = RefCell::new(vec![10, 20]);
    {
        let first = cell.borrow();
        assert_eq!(first.len(), 2);
        assert_eq!(first[1], 20);
        assert_eq!(shared(&first), 30);
        let cloned = Ref::clone(&first);
        drop(first);
        assert!(cell.try_borrow_mut().is_err());
        {
            let nested = cell.borrow();
            assert_eq!(nested.len(), cloned.len());
        }
        assert!(cell.try_borrow_mut().is_err());
        println!("shared {} {}", cloned.len(), shared(&cloned));
    }
    assert!(cell.try_borrow_mut().is_ok());
    {
        let mut guard = cell.borrow_mut();
        assert!(cell.try_borrow().is_err());
        assert!(cell.try_borrow_mut().is_err());
        guard.push(25);
        guard[0] += 1;
        {
            let reborrow: &mut Vec<i32> = &mut guard;
            mutable(reborrow);
            reborrow[1] += 2;
        }
        let reborrow: &Vec<i32> = &guard;
        assert_eq!(shared(reborrow), 88);
        println!("mutable {} {}", guard.len(), shared(&guard));
    }
    assert!(cell.try_borrow().is_ok());
    {
        let mut guard = cell.borrow_mut();
        guard.push(40);
    }
    let guard = cell.borrow();
    println!("released {} {}", guard.len(), shared(&guard));
    assert_eq!(guard.len(), 5);
    assert_eq!(shared(&guard), 128);
}
