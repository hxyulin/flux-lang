use std::sync::{Arc, Weak};

trait Array {
    type Item;
}
impl<T, const N: usize> Array for [T; N] {
    type Item = T;
}

struct Small<A: Array>(Vec<A::Item>);
struct Buffer(Small<[Box<dyn FnOnce()>; 1]>);
struct Resources<T>(Vec<Weak<T>>);

impl<T> Resources<T> {
    fn iter(&self) -> impl Iterator<Item = &Weak<T>> + '_ {
        self.0.iter()
    }
}

struct Tracker(Resources<Buffer>);
impl Tracker {
    fn buffers(&self) -> impl Iterator<Item = &Weak<Buffer>> + '_ {
        self.0.iter()
    }
}

fn main() {
    let callback: Box<dyn FnOnce()> = Box::new(|| println!("callback"));
    let buffer = Arc::new(Buffer(Small(vec![callback])));
    let tracker = Tracker(Resources(vec![Arc::downgrade(&buffer)]));
    println!("{}", tracker.buffers().count());
    let buffer = Arc::try_unwrap(buffer).ok().unwrap();
    for callback in buffer.0.0 {
        callback();
    }
}
