use std::sync::{Arc, Weak};

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
    let weak = Arc::downgrade(&value);
    let other_weak = Arc::downgrade(&shared);
    let pointer = payload(&value);
    assert!(pointer == &*value as *const u32);
    assert!(pointer == Arc::as_ptr(&shared));
    assert!(weak.ptr_eq(&other_weak));
    assert!(weak.as_ptr() == pointer);
    assert!(!weak.ptr_eq(&Arc::downgrade(&Arc::new(37u32))));
    let empty_weak = Weak::new();
    assert!(empty_weak.ptr_eq(&Weak::new()));
    assert!(!empty_weak.ptr_eq(&weak));
    assert!(empty_weak.as_ptr() == Weak::new().as_ptr());
    assert_eq!(Arc::strong_count(&value), 2);
    drop(value);
    let loaded = unsafe { *pointer };
    assert_eq!(loaded, 37);
    drop(shared);
    assert!(weak.upgrade().is_none());
    assert!(weak.ptr_eq(&other_weak));
    assert!(weak.as_ptr() == pointer);
    let slice: Arc<[u32]> = Arc::new([11, 22, 33]);
    let raw = payload(&slice);
    let weak_slice = Arc::downgrade(&slice);
    assert_eq!(weak_slice.as_ptr().len(), 3);
    assert_eq!(weak_slice.as_ptr().cast::<u32>(), raw.cast::<u32>());
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
    println!("{} {} {}", loaded, unsafe { (&*raw)[1] }, unsafe { (&*raw_object).read() });
}
