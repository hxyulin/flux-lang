trait Size {
    type Storage<T>;
}
struct Two;
impl Size for Two {
    type Storage<T> = [T; 2];
}
struct Array<T, S: Size>(S::Storage<T>);
impl<T, S: Size<Storage<T> = [T; N]>, const N: usize> Array<T, S> {
    fn from_array(x: [T; N]) -> Self {
        Self(x)
    }
    fn into_array(self) -> [T; N] {
        self.0
    }
    fn view(&self) -> &[T; N] {
        &self.0
    }
    fn view_mut(&mut self) -> &mut [T; N] {
        &mut self.0
    }
}
trait Identity {
    type Item<T>;
}
struct Id;
impl Identity for Id {
    type Item<T> = T;
}
fn same<T, S: Identity<Item<T> = T>>(x: S::Item<T>) -> T {
    x
}
trait Pair {
    type Items<A, B>;
}
struct Reverse;
impl Pair for Reverse {
    type Items<A, B> = (B, A);
}
fn pair<A, B, S: Pair<Items<A, B> = (B, A)>>(x: S::Items<A, B>) -> (B, A) {
    x
}
trait ConstFamily {
    type Value<T, const N: usize>;
}
struct ByLength;
impl ConstFamily for ByLength {
    type Value<T, const N: usize> = [T; N];
}
fn by_length<T, S: ConstFamily<Value<T, N> = [T; N]>, const N: usize>(x: S::Value<T, N>) -> [T; N] {
    x
}
use std::sync::atomic::{AtomicUsize, Ordering};
static DROPS: AtomicUsize = AtomicUsize::new(0);
struct Owned(u32);
impl Drop for Owned {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
}
fn main() {
    let mut a = Array::<u32, Two>::from_array([3, 5]);
    assert_eq!(a.view()[1], 5);
    a.view_mut()[0] = 7;
    assert_eq!(a.into_array(), [7, 5]);
    let x = 11u32;
    let y = 13u64;
    assert_eq!(*same::<&u32, Id>(&x), 11);
    let p = pair::<&u32, &u64, Reverse>((&y, &x));
    assert_eq!(*p.0, 13);
    assert_eq!(*p.1, 11);
    assert_eq!(by_length::<u32, ByLength, 3>([2, 3, 5]), [2, 3, 5]);
    {
        let owned = Array::<Owned, Two>::from_array([Owned(17), Owned(19)]);
        assert_eq!(owned.view()[0].0 + owned.view()[1].0, 36);
    }
    assert_eq!(DROPS.load(Ordering::SeqCst), 2);
    let refs = Array::<&u32, Two>::from_array([&x, &x]);
    assert_eq!(**refs.view().first().unwrap(), 11);
    println!(
        "gat equalities {} {} {}",
        *p.0,
        *p.1,
        DROPS.load(Ordering::SeqCst)
    );
}
