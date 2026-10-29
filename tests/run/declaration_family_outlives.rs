trait Raw {
    fn add(&mut self, value: u32);
}
trait Lock {
    type Write<'w>: Raw + 'w
    where
        Self: 'w;
    fn lock(&mut self) -> Self::Write<'_>;
}
impl<T: Lock + ?Sized> Lock for &mut T {
    type Write<'w>
        = T::Write<'w>
    where
        Self: 'w;
    fn lock(&mut self) -> Self::Write<'_> {
        (**self).lock()
    }
}
impl<T: Lock + ?Sized> Lock for Box<T> {
    type Write<'w>
        = T::Write<'w>
    where
        Self: 'w;
    fn lock(&mut self) -> Self::Write<'_> {
        (**self).lock()
    }
}
struct Sink(u32);
struct Writer<'a>(&'a mut Sink);
impl Raw for Writer<'_> {
    fn add(&mut self, value: u32) {
        self.0.0 += value;
    }
}
impl Lock for Sink {
    type Write<'w> = Writer<'w>;
    fn lock(&mut self) -> Writer<'_> {
        Writer(self)
    }
}
fn emit<T: Lock>(output: &mut T, value: u32) {
    output.lock().add(value);
}
fn main() {
    let mut sink = Sink(1);
    emit(&mut &mut sink, 7);
    emit(&mut &mut sink, 11);
    let mut boxed = Box::new(Sink(20));
    emit(&mut boxed, 13);
    assert_eq!(sink.0, 19);
    assert_eq!(boxed.0, 33);
    println!("family {} {}", sink.0, boxed.0);
}
