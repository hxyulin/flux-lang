use std::any::Any;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
static DROPS: AtomicUsize = AtomicUsize::new(0);
#[repr(align(128))]
struct Value(u32);
impl Drop for Value {
    fn drop(&mut self) {
        DROPS.fetch_add(self.0 as usize, Ordering::SeqCst);
    }
}
trait Root: 'static {
    fn number(&self) -> u32;
}
trait Child: Root {}
impl Root for Value {
    fn number(&self) -> u32 {
        self.0
    }
}
impl Child for Value {}
fn widen<'a>(x: Box<dyn Any + 'a>) -> Box<dyn Any> {
    x
}
fn widen_arc<'a>(x: Arc<dyn Any + Send + Sync + 'a>) -> Arc<dyn Any + Send + Sync> {
    x
}
fn widen_root<'a>(x: Box<dyn Child + 'a>) -> Box<dyn Root> {
    x
}
fn widen_mut<'a, 'b>(x: &'b mut Box<dyn Any + 'a>) -> &'b mut Box<dyn Any> {
    x
}
fn widen_cell<'a>(x: std::cell::Cell<Box<dyn Root + 'a>>) -> std::cell::Cell<Box<dyn Root>> {
    x
}
fn input_fact<'a>(x: Box<dyn Any + 'a>, r: &'a u32) -> (&'static u32, Box<dyn Any>) {
    (r, x)
}
fn main() {
    let boxed: Box<dyn Any> = Box::new(Value(3));
    let address = &*boxed as *const dyn Any as *const ();
    let boxed = widen(boxed);
    assert_eq!(&*boxed as *const dyn Any as *const (), address);
    assert_eq!(boxed.downcast_ref::<Value>().unwrap().0, 3);
    let mut boxed = boxed;
    let view = widen_mut(&mut boxed);
    view.downcast_mut::<Value>().unwrap().0 = 5;
    drop(boxed);
    assert_eq!(DROPS.load(Ordering::SeqCst), 5);
    let shared: Arc<dyn Any + Send + Sync> = Arc::new(Value(7));
    let address = &*shared as *const (dyn Any + Send + Sync) as *const ();
    let weak = Arc::downgrade(&shared);
    let other = shared.clone();
    let shared = widen_arc(shared);
    assert_eq!(Arc::strong_count(&shared), 2);
    assert_eq!(
        &*shared as *const (dyn Any + Send + Sync) as *const (),
        address
    );
    let typed = shared.downcast::<Value>().ok().unwrap();
    drop(other);
    assert!(weak.upgrade().is_some());
    assert_eq!(typed.0, 7);
    drop(typed);
    assert!(weak.upgrade().is_none());
    assert_eq!(DROPS.load(Ordering::SeqCst), 12);
    let root = widen_root(Box::new(Value(11)));
    assert_eq!(root.number(), 11);
    drop(root);
    assert_eq!(DROPS.load(Ordering::SeqCst), 23);
    let cell: std::cell::Cell<Box<dyn Root>> = std::cell::Cell::new(Box::new(Value(13)));
    let cell = widen_cell(cell);
    let root = cell.into_inner();
    assert_eq!(root.number(), 13);
    drop(root);
    assert_eq!(DROPS.load(Ordering::SeqCst), 36);
    static NUMBER: u32 = 41;
    let (reference, owner) = input_fact(Box::new(4u32), &NUMBER);
    assert_eq!(*reference, 41);
    assert!(owner.is::<u32>());
    println!("objects {} {}", DROPS.load(Ordering::SeqCst), reference);
}
