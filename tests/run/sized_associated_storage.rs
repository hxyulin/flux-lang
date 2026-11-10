trait Layout {
    type Storage: ?Sized;
}
struct Storage<T: ?Sized + Layout>(T::Storage);
impl<T: ?Sized + Layout> Storage<T> {
    fn bytes(&self) -> usize
    where
        Self: Sized,
    {
        std::mem::size_of::<Self>()
    }
}
struct Value;
impl Layout for Value {
    type Storage = u32;
}
fn main() {
    assert_eq!(Storage::<Value>(7).bytes(), 4);
}
