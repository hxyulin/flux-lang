trait Access<'de>: Sized {
    type Output;
    fn take(self) -> Self::Output;
}

struct Wrap<'a, X> {
    delegate: X,
    path: &'a usize,
}

impl<'a, 'de, X: Access<'de>> Access<'de> for Wrap<'a, X> {
    type Output = Wrap<'a, X::Output>;

    fn take(self) -> Self::Output {
        let path = *self.path;
        Wrap {
            delegate: self.delegate.take(),
            path: &path,
        }
    }
}

fn main() {}
