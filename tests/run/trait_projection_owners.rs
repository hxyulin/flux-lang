trait Source<'a> {
    type Out;
    fn get(&self) -> Self::Out;
}

struct Borrow<'a>(&'a u32);

impl<'a> Source<'a> for Borrow<'a> {
    type Out = &'a u32;

    fn get(&self) -> Self::Out {
        self.0
    }
}

struct Wrap<S>(S);

impl<'a, S: Source<'a>> Source<'a> for Wrap<S> {
    type Out = S::Out;

    fn get(&self) -> Self::Out {
        self.0.get()
    }
}

trait Take<T> {
    fn take(&self, value: T) -> T;
}

impl<'a, S: Source<'a>> Take<S::Out> for Wrap<S> {
    fn take(&self, value: S::Out) -> S::Out {
        value
    }
}

fn copy_out<'a, S: Source<'a>>(source: &S) -> S::Out
where
    S::Out: Copy,
{
    source.get()
}

fn main() {
    let left = 17;
    let right = 25;
    let a = Wrap(Wrap(Borrow(&left)));
    let b = Wrap(Borrow(&right));
    let x = a.take(copy_out(&a));
    let y = b.take(copy_out(&b));
    assert!(core::ptr::eq(x, &left));
    assert!(core::ptr::eq(y, &right));
    println!("projection owners {} {} {}", x, y, x + y);
}
