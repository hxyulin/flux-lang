use std::pin::Pin;

struct Pair<T: ?Sized, A>(Box<T>, A);

impl<T: ?Sized, A: 'static> Unpin for Pair<T, A> {}

fn pin_pair<T: ?Sized, A: 'static>(pair: &mut Pair<T, A>) {
    let pinned = Pin::new(pair);
    let _ = pinned.get_mut();
}

trait Method {
    fn value<T: 'static>(value: T) -> T;
}

impl Method for () {
    fn value<T: 'static>(value: T) -> T
    where
        Pair<u32, T>: Unpin,
    {
        value
    }
}

fn main() {
    let value = 42u32;
    let mut borrowed = Pair(Box::new(&value), 0u8);
    pin_pair(&mut borrowed);
    assert_eq!(**borrowed.0, 42);
    let mut pinned = Pair(Box::new(std::marker::PhantomPinned), 0u8);
    pin_pair(&mut pinned);
    println!("{}", <() as Method>::value(value));
}
