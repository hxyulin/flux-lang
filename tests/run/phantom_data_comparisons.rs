use std::cmp::Ordering;
use std::marker::PhantomData;

// No comparison traits: the marker's implementations must not require them.
struct Unordered;

fn compare<T: ?Sized>() {
    let a = PhantomData::<T>;
    let b = PhantomData::<T>;
    assert!(a == b);
    assert!(!(a != b));
    assert!(a <= b);
    assert!(a >= b);
    assert!(!(a < b));
    assert!(!(a > b));
    assert_eq!(a.partial_cmp(&b), Some(Ordering::Equal));
    assert_eq!(a.cmp(&b), Ordering::Equal);
}

// Mirrors ArenaMap's derives without requiring Eq for the marked result.
#[derive(PartialEq, Eq)]
struct MapMarker<T> {
    slot: u32,
    marker: PhantomData<fn() -> T>,
}

fn require_eq<T: Eq>(_: &T) {}

fn main() {
    compare::<Unordered>();
    compare::<str>();
    compare::<[Unordered]>();
    compare::<fn() -> Unordered>();
    compare::<&mut Unordered>();
    let a = MapMarker::<u8> { slot: 3, marker: PhantomData };
    let b = MapMarker::<u8> { slot: 4, marker: PhantomData };
    assert!(a != b);
    require_eq(&PhantomData::<Unordered>);
    println!("marker comparisons: equal; map slots: {} {}", a.slot, b.slot);
}
