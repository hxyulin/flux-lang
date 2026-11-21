struct Guard<'a, T, U, F>
where
    F: Fn(&mut T) -> &mut U,
    U: Default,
{
    source: &'a mut T,
    value: U,
    select: F,
}

impl<'a, T, U, F> Drop for Guard<'a, T, U, F>
where
    F: Fn(&mut T) -> &mut U,
    U: Default,
{
    fn drop(&mut self) {
        *(self.select)(self.source) = std::mem::take(&mut self.value);
    }
}
impl<'a, T, U, F> Guard<'a, T, U, F>
where
    F: Fn(&mut T) -> &mut U,
    U: Default,
{
    fn new(source: &'a mut T, select: F) -> Self {
        let value = std::mem::take(select(source));
        Self { source, value, select }
    }
}
fn main() {
    let mut values = (1, 7);
    drop(Guard::new(&mut values, |value| &mut value.1));
    println!("{:?}", values);
}
