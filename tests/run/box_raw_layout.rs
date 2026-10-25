use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Payload<'a> { value: String, drops: &'a Cell<u32> }
impl Drop for Payload<'_> {
    fn drop(&mut self) {
        assert!(self.value.len() != 0);
        self.drops.set(self.drops.get() + 1);
    }
}
#[repr(align(64))]
struct Aligned<'a> { value: u32, drops: &'a Cell<u32> }
impl Drop for Aligned<'_> {
    fn drop(&mut self) { self.drops.set(self.drops.get() + 1); }
}
trait Read { fn read(&self) -> u32; }
static DYNAMIC_DROPS: AtomicUsize = AtomicUsize::new(0);
#[repr(align(64))]
struct Dynamic { value: u32 }
impl Read for Dynamic { fn read(&self) -> u32 { self.value } }
impl Drop for Dynamic {
    fn drop(&mut self) { DYNAMIC_DROPS.fetch_add(1, Ordering::Relaxed); }
}
#[repr(C)]
struct Header<T: ?Sized> { tag: u8, tail: T }
static ZST_DROPS: AtomicUsize = AtomicUsize::new(0);
#[repr(align(64))]
struct Zst;
impl Drop for Zst {
    fn drop(&mut self) { ZST_DROPS.fetch_add(1, Ordering::Relaxed); }
}
fn main() {
    assert_eq!(std::mem::size_of::<Box<u64>>(), std::mem::size_of::<*mut u64>());
    assert_eq!(std::mem::size_of::<Box<[u64]>>(), std::mem::size_of::<*mut [u64]>());
    assert_eq!(std::mem::size_of::<Box<dyn Read>>(), std::mem::size_of::<*mut dyn Read>());
    let drops = Cell::new(0);
    let moved = *Box::new(Payload { value: String::from("moved"), drops: &drops });
    assert_eq!(drops.get(), 0);
    assert_eq!(moved.value, "moved");
    drop(moved);
    assert_eq!(drops.get(), 1);
    {
        let raw = Box::into_raw(Box::new(Payload { value: String::from("raw"), drops: &drops }));
        let boxed = unsafe { Box::from_raw(raw) };
        assert_eq!(boxed.value, "raw");
    }
    assert_eq!(drops.get(), 2);
    {
        let items: Box<[Payload<'_>]> = vec![
            Payload { value: String::from("one"), drops: &drops },
            Payload { value: String::from("two"), drops: &drops },
        ].into_boxed_slice();
        assert_eq!(items[1].value, "two");
    }
    assert_eq!(drops.get(), 4);
    let aligned_drops = Cell::new(0);
    {
        let value: Box<dyn Read> = Box::new(Dynamic { value: 37 });
        assert_eq!(value.read(), 37);
        assert_eq!(std::mem::align_of_val(&*value), 64);
    }
    assert_eq!(DYNAMIC_DROPS.load(Ordering::Relaxed), 1);
    let raw: *mut dyn Read = Box::into_raw(Box::new(Dynamic { value: 41 }));
    let before = unsafe { std::alloc::Layout::for_value_raw(raw) };
    unsafe { std::ptr::drop_in_place(raw); }
    use std::alloc::Layout as L;
    let after = unsafe { L::for_value_raw(raw) };
    assert_eq!(before.size(), after.size());
    assert_eq!(after.size(), 64);
    assert_eq!(after.align(), 64);
    unsafe { std::alloc::dealloc(raw as *mut u8, after); }
    assert_eq!(DYNAMIC_DROPS.load(Ordering::Relaxed), 2);
    let query: unsafe fn(*const u64) -> L = L::for_value_raw::<u64>;
    let layout = unsafe { query(std::ptr::null::<u64>()) };
    assert_eq!(layout.size(), 8);
    assert_eq!(layout.align(), 8);
    {
        let value: Box<Header<[Aligned<'_>]>> = Box::new(Header {
            tag: 7,
            tail: [Aligned { value: 11, drops: &aligned_drops }, Aligned { value: 13, drops: &aligned_drops }],
        });
        assert_eq!(value.tag, 7);
        assert_eq!(value.tail[1].value, 13);
        assert_eq!(std::mem::align_of_val(&*value), 64);
        assert_eq!(std::mem::size_of_val(&*value), 192);
        let raw = Box::into_raw(value);
        drop(unsafe { Box::from_raw(raw) });
    }
    assert_eq!(aligned_drops.get(), 2);
    drop(Box::new(Zst));
    let zeroes: Box<[Zst]> = vec![Zst, Zst, Zst].into_boxed_slice();
    assert_eq!(zeroes.len(), 3);
    assert_eq!(std::mem::size_of_val(&*zeroes), 0);
    assert_eq!(std::mem::align_of_val(&*zeroes), 64);
    drop(zeroes);
    assert_eq!(ZST_DROPS.load(Ordering::Relaxed), 4);
    let empty: Box<[Aligned<'_>]> = Vec::new().into_boxed_slice();
    assert_eq!(std::mem::align_of_val(&*empty), 64);
    drop(empty);
    let text: Box<str> = String::from("raw λ layout").into_boxed_str();
    let raw = Box::into_raw(text);
    let text = unsafe { Box::from_raw(raw) };
    assert_eq!(&*text, "raw λ layout");
    drop(text);
    println!("box layout {} {} {}", drops.get(), aligned_drops.get(), ZST_DROPS.load(Ordering::Relaxed));
}
