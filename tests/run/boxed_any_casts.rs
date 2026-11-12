use std::any::Any;
use std::sync::atomic::{AtomicUsize, Ordering};
static DROPS: AtomicUsize = AtomicUsize::new(0);
struct Owned(u32);
impl Drop for Owned { fn drop(&mut self) { DROPS.fetch_add(1, Ordering::Relaxed); } }
fn main() {
    let value: Box<dyn Any + Send + Sync> = Box::new(Owned(31));
    let pointer = &*value as *const _ as *const ();
    let value = value as Box<dyn Any + 'static>;
    assert_eq!(&*value as *const _ as *const (), pointer);
    let value = match value.downcast::<u32>() { Ok(_) => panic!("wrong type"), Err(value) => value };
    let value = match value.downcast::<Owned>() { Ok(value) => value, Err(_) => panic!("missing type") };
    assert_eq!(value.0, 31); drop(value);
    let value: Box<dyn Any + Send + Sync> = Box::new(17u32);
    assert_eq!(*value.downcast::<u32>().unwrap(), 17);
    let value: Box<dyn Any + Send> = Box::new(23u32);
    assert_eq!(*value.downcast::<u32>().unwrap(), 23);
    assert_eq!(DROPS.load(Ordering::Relaxed), 1);
    println!("boxed Any casts passed");
}
