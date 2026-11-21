fn require_static<T: 'static>(value: T) -> T {
    value
}

trait Parallel<I: Iterator> {
    fn first(mut batch: I) -> Option<I::Item>
    where
        I::Item: 'static,
    {
        require_static(batch.next())
    }
}

impl<I: Iterator> Parallel<I> for () {}

fn main() {
    let items = [42_u8].into_iter();
    assert_eq!(<() as Parallel<_>>::first(items), Some(42));
    static VALUE: u8 = 42;
    let references = Some(&VALUE).into_iter();
    assert_eq!(<() as Parallel<_>>::first(references), Some(&42));
}
