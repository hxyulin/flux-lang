use std::rc::Rc;
use std::sync::Arc;

trait Callback {
    fn call(&self) -> u8;
}

struct Value(u8);
impl Callback for Value {
    fn call(&self) -> u8 { self.0 }
}

fn push(callbacks: &mut Vec<Rc<dyn Callback>>, value: Box<dyn Callback>) {
    callbacks.push(Rc::from(value));
}

fn main() {
    let mut callbacks = Vec::new();
    push(&mut callbacks, Box::new(Value(42)));

    let boxed: Box<dyn Callback> = Box::new(Value(11));
    let shared: Arc<dyn Callback> = Arc::from(boxed);
    let from_sized: Rc<dyn Callback> = Rc::from(Value(7));
    let slice: &[u8] = &[2, 3];
    let copied: Rc<[u8]> = Rc::from(slice);

    println!("{} {} {} {:?}", callbacks[0].call(), shared.call(), from_sized.call(), &*copied);
}
