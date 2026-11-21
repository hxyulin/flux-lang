use std::ptr::{self, NonNull};

trait Value {
    fn value(&self) -> u32;
}

impl Value for u32 {
    fn value(&self) -> u32 {
        *self
    }
}

fn boxed<T: Value + 'static>(value: T) -> Option<Box<dyn Value>> {
    Some(Box::new(value) as _)
}

static STORED: u8 = 33;
const NULL: Option<NonNull<u8>> = NonNull::new(ptr::null_mut());
const ADDRESS: Option<NonNull<u8>> = NonNull::new(ptr::without_provenance_mut(17));
const STATIC_NON_NULL: NonNull<u8> = NonNull::new((&raw const STORED).cast_mut()).unwrap();
const WIDE_NULL: bool = ptr::slice_from_raw_parts(ptr::null::<u8>(), 0).is_null();

fn main() {
    assert!(NULL.is_none());
    assert_eq!(ADDRESS.unwrap().as_ptr() as usize, 17);
    assert!(WIDE_NULL);
    assert!(ptr::slice_from_raw_parts(ptr::null::<u8>(), 5).is_null());
    assert!(!ptr::slice_from_raw_parts(&STORED, 0).is_null());
    assert!(NonNull::new(ptr::null_mut::<u8>()).is_none());
    println!("{} {}", boxed(9u32).unwrap().value(), unsafe {
        *STATIC_NON_NULL.as_ptr()
    });
}
