use std::any::Any;
#[derive(Debug)]
struct Owned(i32);
impl Drop for Owned {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}
fn main() {
    let value: Box<dyn Any + Send> = Box::new(Owned(17));
    let address = value.downcast_ref::<Owned>().unwrap() as *const Owned as usize;
    let value = value.downcast::<u32>().err().unwrap();
    assert_eq!(
        value.downcast_ref::<Owned>().unwrap() as *const Owned as usize,
        address
    );
    let value = value.downcast::<Owned>().unwrap();
    assert_eq!(&*value as *const Owned as usize, address);
    println!("{}", value.0);
    drop(value);

    let original = std::rc::Rc::new(Owned(29));
    let erased: std::rc::Rc<dyn Any> = original.clone();
    let erased = erased.downcast::<i32>().err().unwrap();
    let restored = erased.downcast::<Owned>().ok().unwrap();
    assert_eq!(
        std::rc::Rc::as_ptr(&restored),
        std::rc::Rc::as_ptr(&original)
    );
    assert_eq!(std::rc::Rc::strong_count(&restored), 2);
}
