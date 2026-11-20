use std::cell::RefCell;
use std::collections::VecDeque;

fn main() {
    let value = RefCell::new(Some(2));
    if let Ok(Some(number)) = value.try_borrow_mut().as_deref_mut() {
        *number += 5;
    }
    let mut error: Result<Box<u32>, i32> = Err(3);
    *error.as_deref_mut().unwrap_err() += 2;
    println!("{:?} {:?}", value.borrow().as_ref(), error);

    let mut deque = VecDeque::with_capacity(4);
    deque.extend(0..4);
    deque.pop_front();
    deque.pop_front();
    deque.extend(4..6);
    println!("{} {} {}", deque.contains(&2), deque.contains(&5), deque.contains(&0));
}
