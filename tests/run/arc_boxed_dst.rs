use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

static DROPS: AtomicUsize = AtomicUsize::new(0);

trait Value {
    fn get(&self) -> usize;
}

#[repr(align(128))]
struct Heap(Box<usize>);

impl Value for Heap {
    fn get(&self) -> usize { *self.0 }
}

impl Drop for Heap {
    fn drop(&mut self) { DROPS.fetch_add(1, Ordering::SeqCst); }
}

#[repr(align(512))]
struct Empty;
impl Value for Empty {
    fn get(&self) -> usize { 17 }
}

fn main() {
    let boxed: Box<dyn Value> = Box::new(Heap(Box::new(42)));
    let value: Arc<dyn Value> = Arc::<dyn Value>::from(boxed);
    assert_eq!(Arc::as_ptr(&value) as *const u8 as usize % 128, 0);
    let weak = Arc::downgrade(&value);
    let owner = weak.upgrade().unwrap();
    drop(value);
    assert_eq!(DROPS.load(Ordering::SeqCst), 0);
    println!("{}", owner.get());
    drop(owner);
    println!("{} {}", DROPS.load(Ordering::SeqCst), weak.upgrade().is_none());

    let empty: Box<dyn Value> = Box::new(Empty);
    let empty: Arc<dyn Value> = Arc::<dyn Value>::from(empty);
    assert_eq!(Arc::as_ptr(&empty) as *const u8 as usize % 512, 0);
    println!("{}", empty.get());

    let slice: Arc<[String]> = Arc::<[String]>::from(vec![String::from("contents")].into_boxed_slice());
    let text: Arc<str> = Arc::<str>::from(Box::<str>::from("string"));
    let sized: Arc<usize> = Arc::<usize>::from(Box::new(13usize));
    println!("{} {} {}", slice[0], &*text, *sized);
}
