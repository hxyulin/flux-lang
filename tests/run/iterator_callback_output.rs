struct Once<T>(T);

impl<T> Iterator for Once<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        None
    }

    fn for_each<F>(self, mut f: F) -> F::Output
    where
        F: FnMut(T),
    {
        f(self.0)
    }
}

fn main() {
    Once(42).for_each(|value| println!("{value}"));
}
