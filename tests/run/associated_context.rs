use std::hash::{BuildHasher, RandomState};

struct Map<K, V, S = RandomState> {
    pairs: Vec<(K, V)>,
    hasher: S,
}
impl<K, V> Map<K, V> {
    fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hasher(capacity, <_>::default())
    }
}
impl<K, V, S> Map<K, V, S> {
    fn with_capacity_and_hasher(capacity: usize, hasher: S) -> Self {
        Self {
            pairs: Vec::with_capacity(capacity),
            hasher,
        }
    }
}
impl<K, V, S: BuildHasher + Default> FromIterator<(K, V)> for Map<K, V, S> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iterable: I) -> Self {
        let iter = iterable.into_iter();
        let (low, _) = iter.size_hint();
        let mut map = Self::with_capacity_and_hasher(low, <_>::default());
        map.pairs.extend(iter);
        map
    }
}
trait Make {
    fn make() -> Self;
}
struct Owned(String);
impl Make for Owned {
    fn make() -> Self {
        Self(String::from("owned"))
    }
}
fn make<T: Make>() -> T {
    <_>::make()
}
fn take_owned(value: Owned) -> String {
    value.0
}
trait Optional: Sized {
    fn optional() -> Option<Self>;
}
impl Optional for u32 {
    fn optional() -> Option<Self> {
        Some(37)
    }
}
trait Made: Sized {
    const MADE: Self;
}
impl Made for u32 {
    const MADE: Self = 19;
}
fn inferred_argument(value: u32) -> u32 {
    value
}
fn main() {
    let empty = Map::<u32, String>::with_capacity(3);
    assert_eq!(empty.pairs.capacity(), 3);
    let _: RandomState = empty.hasher;
    let map: Map<u32, String> = [(1, String::from("first")), (2, String::from("second"))]
        .into_iter()
        .collect();
    assert_eq!(map.pairs[0].0 + map.pairs[1].0, 3);
    assert_eq!(map.pairs[1].1, "second");
    let owned: Owned = make();
    assert_eq!(owned.0, "owned");
    assert_eq!(take_owned(<_>::make()), "owned");
    let optional: Option<u32> = <_>::optional();
    assert_eq!(optional, Some(37));
    let constant: u32 = <_>::MADE;
    assert_eq!(constant, 19);
    assert_eq!(inferred_argument(<_>::default()), 0);
    println!(
        "associated context {} {} {}",
        map.pairs.len(),
        optional.unwrap(),
        constant
    );
}
