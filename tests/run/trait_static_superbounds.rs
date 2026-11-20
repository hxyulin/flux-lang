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

trait Read<'a, T: Child>: Sized {
    type Output: 'a;

    fn read(self, value: &'a T) -> Self::Output;

    fn optional(self, value: &'a T) -> Option<Self::Output> {
        Some(self.read(value))
    }
}

impl<'a> Read<'a, Owned> for () {
    type Output = &'a Owned;

    fn read(self, value: &'a Owned) -> &'a Owned {
        value
    }
}

trait Write<'a, T: Child>: Read<'a, T> {
    type Mutable: 'a;

    fn write(self, value: &'a mut T) -> Self::Mutable;

    fn optional_mut(self, value: &'a mut T) -> Option<Self::Mutable> {
        Some(self.write(value))
    }
}

impl<'a> Write<'a, Owned> for () {
    type Mutable = &'a mut Owned;

    fn write(self, value: &'a mut Owned) -> &'a mut Owned {
        value
    }
}

struct Dimension<const N: usize>;
trait KnownDimension {}
impl KnownDimension for Dimension<2> {}

struct SizedValue<const N: usize>(u32);

impl<const N: usize> PartialEq for SizedValue<N>
where
    Dimension<N>: KnownDimension,
{
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<const N: usize> Eq for SizedValue<N> where Dimension<N>: KnownDimension {}

fn equal<T: Eq>(left: &T, right: &T) -> bool {
    left == right
}

fn main() {
    assert!(equal(&SizedValue::<2>(7), &SizedValue::<2>(7)));
    assert!(!equal(&SizedValue::<2>(7), &SizedValue::<2>(9)));
    assert_eq!(Little.read([0x34, 0x12]), 0x1234);
    assert_eq!(echo(31u32), 31);
    let mut owned = Owned(72);
    ().optional_mut(&mut owned).unwrap().0 += 1;
    assert_eq!(().optional(&owned).unwrap().checked(), 73);
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
