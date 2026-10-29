use core::cell::Cell;
use core::sync::atomic::{AtomicUsize, Ordering};
static DROPS: AtomicUsize = AtomicUsize::new(0);
struct D;
impl Drop for D {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::Relaxed);
    }
}
enum E {
    None,
    Owned(D),
    Mutable(Cell<u32>),
    Text(String),
}
#[repr(align(128))]
struct Aligned {
    value: E,
    bytes: [u8; 3],
}
fn named() -> &'static E {
    const DEFAULT: E = E::None;
    &DEFAULT
}
fn literal() -> &'static E {
    &E::None
}
fn array() -> &'static [E; 2] {
    &[E::None, E::None]
}
fn aligned() -> &'static Aligned {
    const DATA: Aligned = Aligned {
        value: E::None,
        bytes: [3, 7, 11],
    };
    &DATA
}
fn projected() -> &'static u32 {
    const DATA: (E, u32) = (E::None, 73);
    &DATA.1
}
fn is_none(value: &E) -> bool {
    matches!(value, E::None)
}
fn main() {
    assert!(is_none(named()) && is_none(literal()));
    assert!(array().iter().all(is_none));
    assert!(is_none(&aligned().value));
    assert_eq!((aligned() as *const Aligned as usize) % 128, 0);
    assert_eq!(aligned().bytes, [3, 7, 11]);
    assert_eq!(*projected(), 73);
    assert_eq!(DROPS.load(Ordering::Relaxed), 0);
    {
        let _active = E::Owned(D);
    }
    assert_eq!(DROPS.load(Ordering::Relaxed), 1);
    let cell = E::Mutable(Cell::new(19));
    if let E::Mutable(value) = cell {
        assert_eq!(value.get(), 19);
    }
    let text = E::Text(String::from("owned"));
    if let E::Text(value) = text {
        assert_eq!(value, "owned");
    }
    println!(
        "{} {} {}",
        aligned().bytes[1],
        projected(),
        DROPS.load(Ordering::Relaxed)
    );
}
