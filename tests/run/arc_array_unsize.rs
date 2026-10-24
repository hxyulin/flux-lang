use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
struct Owner<'a>(&'a Cell<usize>, usize);
impl Drop for Owner<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() * 10 + self.1);
    }
}
#[repr(align(64))]
struct Aligned(u8);
fn slice<const N: usize>(array: Arc<[u8; N]>) -> Arc<[u8]> {
    array
}
static ZST_DROPS: AtomicUsize = AtomicUsize::new(0);
struct Zst;
impl Drop for Zst {
    fn drop(&mut self) {
        ZST_DROPS.fetch_add(1, Ordering::SeqCst);
    }
}
fn moved_length(value: Arc<[u8]>) -> usize {
    value.len()
}
fn choose(flag: bool) -> Arc<[u8]> {
    if flag {
        Arc::new([1, 2])
    } else {
        Arc::new([3])
    }
}
fn main() {
    let zst_array = Arc::new([Zst, Zst, Zst]);
    let zst_weak = Arc::downgrade(&zst_array);
    let zsts: Arc<[Zst]> = zst_array;
    let zst_copy = zsts.clone();
    drop(zsts);
    assert_eq!(ZST_DROPS.load(Ordering::SeqCst), 0);
    drop(zst_copy);
    assert_eq!(ZST_DROPS.load(Ordering::SeqCst), 3);
    assert!(zst_weak.upgrade().is_none());
    drop(zst_weak);
    assert_eq!(moved_length(Arc::new([1, 2, 3, 4])), 4);
    let optional: Option<Arc<[u8]>> = Some(Arc::new([5, 6]));
    assert_eq!(optional.unwrap().len(), 2);
    assert_eq!(choose(std::hint::black_box(true)).len(), 2);
    assert_eq!(choose(std::hint::black_box(false)).len(), 1);
    let drops = Cell::new(0);
    let array = Arc::new([Owner(&drops, 1), Owner(&drops, 2)]);
    let weak = Arc::downgrade(&array);
    let before = &*array as *const [Owner<'_>; 2] as *const Owner<'_>;
    let shared: Arc<[Owner<'_>]> = array;
    let after = &*shared as *const [Owner<'_>] as *const Owner<'_>;
    assert!(before == after);
    let copy = shared.clone();
    drop(shared);
    let restored = weak.upgrade().unwrap();
    assert!(before == &*restored as *const [Owner<'_>; 2] as *const Owner<'_>);
    drop(restored);
    assert_eq!(drops.get(), 0);
    drop(copy);
    assert_eq!(drops.get(), 12);
    assert!(weak.upgrade().is_none());
    drop(weak);
    let empty: Arc<[String]> = Arc::new([]);
    let aligned: Arc<[Aligned]> = Arc::new([Aligned(9), Aligned(7)]);
    assert_eq!((&aligned[0] as *const Aligned as usize) % 64, 0);
    let generic = slice(Arc::new([3, 4, 5]));
    println!(
        "{} {} {} {} {}",
        drops.get(),
        empty.len(),
        aligned[0].0,
        aligned[1].0,
        generic[2]
    );
}
