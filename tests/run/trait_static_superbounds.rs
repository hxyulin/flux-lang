use std::sync::atomic::{AtomicUsize, Ordering};

trait Endian: std::fmt::Debug + Default + Clone + Copy + PartialEq + Eq + 'static {
    fn read(self, bytes: [u8; 2]) -> u16;
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Little;
impl Endian for Little {
    fn read(self, bytes: [u8; 2]) -> u16 {
        u16::from_le_bytes(bytes)
    }
}
unsafe trait Pod: Copy + 'static {}
unsafe impl Pod for u32 {}
fn echo<T: Pod>(value: T) -> T {
    value
}

// Children deliberately precede parents to exercise order-independent metadata.
trait Child: Parent {
    fn checked(&self) -> u32 {
        fn require_static<T: ?Sized + 'static>() {}
        require_static::<Self>();
        self.value()
    }
}
trait Parent: 'static {
    fn value(&self) -> u32;
}
static DROPS: AtomicUsize = AtomicUsize::new(0);
struct Owned(u32);
impl Parent for Owned {
    fn value(&self) -> u32 {
        self.0
    }
}
impl Child for Owned {}
impl Drop for Owned {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
}
fn boxed<T: Child>(value: T) -> Box<dyn Parent> {
    Box::new(value)
}
fn marker<T: Endian>(value: T) -> Box<dyn std::fmt::Debug> {
    Box::new(value)
}
fn main() {
    assert_eq!(Little.read([0x34, 0x12]), 0x1234);
    assert_eq!(echo(31u32), 31);
    let owned = Owned(73);
    assert_eq!(owned.checked(), 73);
    let value = boxed(owned);
    assert_eq!(value.value(), 73);
    drop(value);
    assert_eq!(DROPS.load(Ordering::SeqCst), 1);
    println!(
        "static {} {} {:?}",
        Little.read([0x34, 0x12]),
        echo(31u32),
        marker(Little)
    );
}
