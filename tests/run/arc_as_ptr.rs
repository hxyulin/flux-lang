use std::sync::Arc;

trait Read {
    fn read(&self) -> u32;
}
impl Read for u32 {
    fn read(&self) -> u32 { *self }
}
fn payload<T: ?Sized>(value: &Arc<T>) -> *const T {
    Arc::as_ptr(value)
}
fn main() {
    let value = Arc::new(37u32);
    let shared = value.clone();
    let pointer = payload(&value);
    assert!(pointer == &*value as *const u32);
    assert!(pointer == Arc::as_ptr(&shared));
    assert_eq!(Arc::strong_count(&value), 2);
    drop(value);
    assert_eq!(unsafe { *pointer }, 37);
    let slice: Arc<[u32]> = Arc::new([11, 22, 33]);
    let raw = payload(&slice);
    assert_eq!(raw.len(), 3);
    assert!(raw as *const u32 == &slice[0] as *const u32);
    assert_eq!(unsafe { (&*raw)[2] }, 33);
    let empty: Arc<[u8]> = Arc::new([]);
    assert_eq!(Arc::as_ptr(&empty).len(), 0);
    let text: Arc<str> = Arc::from(String::from("hello").into_boxed_str());
    let raw_text = Arc::as_ptr(&text);
    assert_eq!(unsafe { (&*raw_text).len() }, 5);
    let object: Arc<dyn Read> = Arc::new(91u32);
    let raw_object = Arc::as_ptr(&object);
    assert_eq!(unsafe { (&*raw_object).read() }, 91);
    println!("{} {} {}", unsafe { *pointer }, unsafe { (&*raw)[1] }, unsafe { (&*raw_object).read() });
}
