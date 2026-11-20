use std::rc::{Rc, Weak};

fn main() {
    let original: Rc<str> = Rc::from("weak metadata");
    let weak = Rc::downgrade(&original);
    let raw = weak.into_raw();
    assert_eq!(Rc::weak_count(&original), 1);
    let weak = unsafe { Weak::from_raw(raw) };
    println!("{}", &*weak.upgrade().unwrap());
    let raw = weak.into_raw();
    drop(original);
    let weak = unsafe { Weak::from_raw(raw) };
    println!("expired: {}", weak.upgrade().is_none());

    let empty = Weak::<u64>::new();
    let empty = unsafe { Weak::from_raw(empty.into_raw()) };
    println!("empty: {}", empty.upgrade().is_none());
}
