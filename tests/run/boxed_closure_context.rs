use std::rc::Rc;
use std::sync::Arc;
struct Event { value: i32 }
impl Event { fn value(&self) -> i32 { self.value } }
fn boxed(callback: Box<dyn Fn(&Event) -> i32>) -> i32 { callback(&Event { value: 7 }) }
fn shared(callback: Rc<dyn Fn(&Event) -> i32>) -> i32 { callback(&Event { value: 11 }) }
fn atomic(callback: Arc<dyn Fn(&Event) -> i32>) -> i32 { callback(&Event { value: 13 }) }
fn main() {
    println!("{} {} {}", boxed(Box::new(|event| event.value())), shared(Rc::new(|event| event.value())), atomic(Arc::new(|event| event.value())));
    let callback: Box<dyn Fn(&Event) -> i32> = Box::new(|event| event.value() * 2);
    println!("{}", callback(&Event { value: 17 }));
}
