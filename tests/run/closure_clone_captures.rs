use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

struct Owned { text: String, clones: Arc<AtomicUsize>, drops: Arc<AtomicUsize> }
impl Owned { fn len(&self) -> usize { self.text.len() } }
impl Clone for Owned {
    fn clone(&self) -> Self {
        self.clones.fetch_add(1, Ordering::SeqCst);
        Self { text: self.text.clone(), clones: self.clones.clone(), drops: self.drops.clone() }
    }
}
impl Drop for Owned {
    fn drop(&mut self) { self.drops.fetch_add(1, Ordering::SeqCst); }
}
#[derive(Copy)]
struct CopyCapture { value: usize, clones: &'static AtomicUsize }
impl CopyCapture { fn value(&self) -> usize { self.value } }
impl Clone for CopyCapture {
    fn clone(&self) -> Self { self.clones.fetch_add(1, Ordering::SeqCst); *self }
}
static COPY_CLONES: AtomicUsize = AtomicUsize::new(0);
fn cloned<F: Clone>(value: &F) -> F { value.clone() }
fn main() {
    let clones = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let value = Owned { text: String::from("hello"), clones: clones.clone(), drops: drops.clone() };
    let closure = move |extra: usize| value.len() + extra;
    let duplicate = cloned(&closure);
    assert_eq!(closure(2), 7);
    assert_eq!(duplicate(4), 9);
    assert_eq!(clones.load(Ordering::SeqCst), 1);
    drop(closure);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(duplicate(0), 5);
    drop(duplicate);
    assert_eq!(drops.load(Ordering::SeqCst), 2);
    let value = CopyCapture { value: 17, clones: &COPY_CLONES };
    let closure = move || value.value();
    let duplicate = cloned(&closure);
    assert_eq!(duplicate(), 17);
    assert_eq!(COPY_CLONES.load(Ordering::SeqCst), 1);
    let text = String::from("borrowed");
    let reference = &text;
    let borrowed = move || reference.len();
    assert_eq!(cloned(&borrowed)(), 8);
    let nested = move || duplicate();
    assert_eq!(cloned(&nested)(), 17);
    assert_eq!(COPY_CLONES.load(Ordering::SeqCst), 2);
    println!("owned, borrowed, Copy, and nested closure captures cloned");
}
