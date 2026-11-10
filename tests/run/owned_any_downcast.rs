use std::any::Any;
#[derive(Debug)]
struct Owned(i32);
impl Drop for Owned { fn drop(&mut self) { println!("drop {}", self.0); } }
fn main() {
    let value: Box<dyn Any + Send> = Box::new(Owned(17));
    let address = value.downcast_ref::<Owned>().unwrap() as *const Owned as usize;
    let value = value.downcast::<u32>().err().unwrap();
    assert_eq!(value.downcast_ref::<Owned>().unwrap() as *const Owned as usize, address);
    let value = value.downcast::<Owned>().unwrap();
    assert_eq!(&*value as *const Owned as usize, address);
    println!("{}", value.0);
    drop(value);
}
