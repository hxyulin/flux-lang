trait Iter: Sized {
    type Item;
    fn wrap(self, item: Self::Item) -> Wrap<Self> {
        Wrap::new(self, item)
    }
}
struct Wrap<I: Iter> {
    base: I,
    item: I::Item,
}
impl<I: Iter> Wrap<I> {
    fn new(base: I, item: I::Item) -> Self {
        Self { base, item }
    }
}
struct Items<'a, T>(&'a [T]);
impl<'a, T: Sync> Iter for Items<'a, T> {
    type Item = &'a T;
}
fn main() {
    let values = vec![Some(1i32)];
    let separator = None;
    let wrapped = Items(&values).wrap(&separator);
    assert!(std::ptr::eq(wrapped.item, &separator));
    assert!(wrapped.item.is_none());
    assert_eq!(wrapped.base.0.len(), 1);
    println!("associated field with auto bound passed");
}
