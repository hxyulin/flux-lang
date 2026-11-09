trait Writer {
    fn put(&mut self, value: u32);
}
impl Writer for Vec<u32> {
    fn put(&mut self, value: u32) {
        self.push(value);
    }
}
impl<T: Writer + ?Sized> Writer for &mut T {
    fn put(&mut self, value: u32) {
        (**self).put(value);
    }
}
trait Lock {
    type Locked<'w>: Writer + 'w where Self: 'w;
    fn locked(&mut self) -> Self::Locked<'_>;
}
impl Lock for dyn Writer {
    type Locked<'w> = &'w mut Self;
    fn locked(&mut self) -> Self::Locked<'_> {
        self
    }
}
trait Offset<T> {}
trait Stream: Offset<Self::Checkpoint> {
    type Checkpoint;
    type Slice;
}
impl<'a, T> Offset<&'a [T]> for &'a [T] {}
impl<T: Clone + std::fmt::Debug> Stream for &[T] {
    type Checkpoint = Self;
    type Slice = Self;
}
trait Update: Stream {
    fn update(self, inner: Self::Slice) -> Self;
}
impl<T: Clone + std::fmt::Debug> Update for &[T] {
    fn update(self, inner: Self::Slice) -> Self {
        inner
    }
}
fn main() {
    let mut values = Vec::new();
    {
        let writer: &mut (dyn Writer + 'static) = &mut values;
        let mut locked = writer.locked();
        locked.put(20);
    }
    let old = [1, 2];
    let replacement = [22];
    let result = Update::update(&old[..], &replacement[..]);
    println!("{}", values[0] + result[0]);
}
